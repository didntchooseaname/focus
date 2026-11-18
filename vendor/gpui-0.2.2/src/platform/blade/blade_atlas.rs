use crate::{
    AtlasKey, AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, DevicePixels, PlatformAtlas,
    Point, Size, platform::AtlasTextureList,
};
use anyhow::Result;
use blade_graphics as gpu;
use blade_util::{BufferBelt, BufferBeltDescriptor};
use collections::FxHashMap;
use etagere::BucketedAtlasAllocator;
use parking_lot::Mutex;
use std::{borrow::Cow, collections::VecDeque, ops, sync::Arc};

pub(crate) struct BladeAtlas(Mutex<BladeAtlasState>);

struct PendingUpload {
    id: AtlasTextureId,
    bounds: Bounds<DevicePixels>,
    data: gpu::BufferPiece,
}

struct BladeAtlasState {
    gpu: Arc<gpu::Context>,
    upload_belt: BufferBelt,
    storage: BladeAtlasStorage,
    tiles_by_key: FxHashMap<AtlasKey, AtlasTile>,
    initializations: Vec<AtlasTextureId>,
    uploads: Vec<PendingUpload>,
    /// Maps the public atlas id to its reusable storage slot.
    ///
    /// An id used to be the slot index itself. Reusing a slot then made a
    /// retained GPUI scene silently point at a different texture; destroying
    /// it made `get_texture_info` unwrap an empty slot. RenderImage frames are
    /// replaced continuously, so this is not an exceptional race. IDs are now
    /// monotonic while slots remain reusable.
    texture_slots: FxHashMap<AtlasTextureId, usize>,
    next_texture_id: u32,
    /// Tiles whose key has been released, waiting for the GPU to be done with
    /// the frame that may still be sampling them.
    ///
    /// Both halves of the release are deferred. Handing a tile's rectangle
    /// back to the allocator lets the next image land on pixels the in-flight
    /// frame is still reading, and destroying a texture the moment its last
    /// tile goes leaves an empty slot that `BladeAtlasStorage`'s `Index`
    /// unwraps, so a scene already handed to the renderer takes the whole
    /// process down with it. Dropping an image while a frame is in flight is
    /// ordinary - a video stream moving between two elements does it on every
    /// frame - so the release waits for the frame's sync point instead.
    retired: Vec<RetiredTile>,
    /// One retirement batch per submitted frame. A batch is reaped only when
    /// BladeRenderer has actually waited for that frame's sync point.
    retired_in_flight: VecDeque<Vec<RetiredTile>>,
}

/// A released tile: which texture held it, and where in that texture.
struct RetiredTile {
    texture: AtlasTextureId,
    tile: crate::platform::TileId,
}

#[cfg(gles)]
unsafe impl Send for BladeAtlasState {}

impl BladeAtlasState {
    fn destroy(&mut self) {
        self.retired.clear();
        self.retired_in_flight.clear();
        self.texture_slots.clear();
        self.storage.destroy(&self.gpu);
        self.upload_belt.destroy(&self.gpu);
    }
}

pub struct BladeTextureInfo {
    pub raw_view: gpu::TextureView,
}

impl BladeAtlas {
    pub(crate) fn new(gpu: &Arc<gpu::Context>) -> Self {
        BladeAtlas(Mutex::new(BladeAtlasState {
            gpu: Arc::clone(gpu),
            upload_belt: BufferBelt::new(BufferBeltDescriptor {
                memory: gpu::Memory::Upload,
                min_chunk_size: 0x10000,
                alignment: 64, // Vulkan `optimalBufferCopyOffsetAlignment` on Intel XE
            }),
            storage: BladeAtlasStorage::default(),
            tiles_by_key: Default::default(),
            initializations: Vec::new(),
            uploads: Vec::new(),
            texture_slots: Default::default(),
            next_texture_id: 0,
            retired: Vec::new(),
            retired_in_flight: VecDeque::new(),
        }))
    }

    pub(crate) fn destroy(&self) {
        self.0.lock().destroy();
    }

    pub fn before_frame(&self, gpu_encoder: &mut gpu::CommandEncoder) {
        let mut lock = self.0.lock();
        lock.flush(gpu_encoder);
    }

    pub fn after_frame(&self, sync_point: &gpu::SyncPoint) {
        let mut lock = self.0.lock();
        lock.upload_belt.flush(sync_point);
        let retired = std::mem::take(&mut lock.retired);
        lock.retired_in_flight.push_back(retired);
    }

    /// The renderer has waited for its oldest submitted frame. Only now may
    /// textures released by that frame be destroyed and their slots reused.
    pub fn gpu_completed(&self) {
        let mut lock = self.0.lock();
        let Some(retired) = lock.retired_in_flight.pop_front() else {
            return;
        };
        lock.reap_retired(retired);
    }

    pub fn get_texture_info(&self, id: AtlasTextureId) -> Option<BladeTextureInfo> {
        let lock = self.0.lock();
        let texture = lock.texture(id)?;
        Some(BladeTextureInfo {
            raw_view: texture.raw_view,
        })
    }
}

impl PlatformAtlas for BladeAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: &AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        let mut lock = self.0.lock();
        if let Some(tile) = lock.tiles_by_key.get(key) {
            Ok(Some(tile.clone()))
        } else {
            profiling::scope!("new tile");
            let Some((size, bytes)) = build()? else {
                return Ok(None);
            };
            let tile = lock.allocate(size, key.texture_kind());
            lock.upload_texture(tile.texture_id, tile.bounds, &bytes);
            lock.tiles_by_key.insert(key.clone(), tile.clone());
            Ok(Some(tile))
        }
    }

    fn remove(&self, key: &AtlasKey) {
        let mut lock = self.0.lock();

        let Some(tile) = lock.tiles_by_key.remove(key) else {
            return;
        };

        // The rectangle stays allocated and the texture stays in place, both
        // valid, until the frame that may be drawing from them has finished
        // on the GPU.
        lock.retired.push(RetiredTile {
            texture: tile.texture_id,
            tile: tile.tile_id,
        });
    }

    fn update(&self, key: &AtlasKey, size: Size<DevicePixels>, bytes: &[u8]) -> bool {
        let mut lock = self.0.lock();
        let Some(tile) = lock.tiles_by_key.get(key).cloned() else {
            return false;
        };
        // A different size needs a different rectangle, which is a new tile.
        if tile.bounds.size != size {
            return false;
        }
        // The copy is queued like any other upload and lands in the same
        // transfer pass, before the frame that will sample it.
        lock.upload_texture(tile.texture_id, tile.bounds, bytes);
        true
    }
}

impl BladeAtlasState {
    fn texture(&self, id: AtlasTextureId) -> Option<&BladeAtlasTexture> {
        let slot = *self.texture_slots.get(&id)?;
        self.storage[id.kind].textures.get(slot)?.as_ref()
    }

    fn fresh_texture_id(&mut self, kind: AtlasTextureKind) -> AtlasTextureId {
        loop {
            let id = AtlasTextureId {
                index: self.next_texture_id,
                kind,
            };
            self.next_texture_id = self.next_texture_id.wrapping_add(1);
            if !self.texture_slots.contains_key(&id) {
                return id;
            }
        }
    }

    /// Hand the released rectangles back to their allocators and destroy the
    /// textures that emptied. Called once the frame's sync point says the GPU
    /// has finished reading from them.
    ///
    /// Reclaiming the rectangle is what keeps a long session's atlas flat:
    /// avatars, emoji and thumbnails come and go by the thousand, and a
    /// texture that only ever grew meant a new 1024² allocation every time the
    /// live ones no longer fitted.
    fn reap_retired(&mut self, retired: Vec<RetiredTile>) {
        for entry in retired {
            let Some(slot) = self.texture_slots.get(&entry.texture).copied() else {
                continue;
            };
            let list = &mut self.storage[entry.texture.kind];
            let Some(texture) = list.textures.get_mut(slot).and_then(|slot| slot.as_mut()) else {
                continue;
            };
            texture.deallocate(entry.tile);
            // A fresh tile can land on the same texture between the release
            // and here, which puts it back in use.
            if !texture.is_unreferenced() {
                continue;
            }
            let Some(mut texture) = list.textures[slot].take() else {
                continue;
            };
            self.texture_slots.remove(&entry.texture);
            list.free_list.push(slot);
            texture.destroy(&self.gpu);
        }
    }

    fn allocate(&mut self, size: Size<DevicePixels>, texture_kind: AtlasTextureKind) -> AtlasTile {
        {
            let textures = &mut self.storage[texture_kind];

            if let Some(tile) = textures
                .iter_mut()
                .rev()
                .find_map(|texture| texture.allocate(size))
            {
                return tile;
            }
        }

        let texture = self.push_texture(size, texture_kind);
        texture.allocate(size).unwrap()
    }

    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> &mut BladeAtlasTexture {
        const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(1024),
            height: DevicePixels(1024),
        };

        let size = min_size.max(&DEFAULT_ATLAS_SIZE);
        let format;
        let usage;
        match kind {
            AtlasTextureKind::Monochrome => {
                format = gpu::TextureFormat::R8Unorm;
                usage = gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE;
            }
            AtlasTextureKind::Polychrome => {
                format = gpu::TextureFormat::Bgra8Unorm;
                usage = gpu::TextureUsage::COPY | gpu::TextureUsage::RESOURCE;
            }
        }

        let raw = self.gpu.create_texture(gpu::TextureDesc {
            name: "atlas",
            format,
            size: gpu::Extent {
                width: size.width.into(),
                height: size.height.into(),
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: gpu::TextureDimension::D2,
            usage,
            external: None,
        });
        let raw_view = self.gpu.create_texture_view(
            raw,
            gpu::TextureViewDesc {
                name: "",
                format,
                dimension: gpu::ViewDimension::D2,
                subresources: &Default::default(),
            },
        );

        let id = self.fresh_texture_id(kind);
        let texture_list = &mut self.storage[kind];
        let slot = texture_list
            .free_list
            .pop()
            .unwrap_or(texture_list.textures.len());

        let atlas_texture = BladeAtlasTexture {
            id,
            allocator: etagere::BucketedAtlasAllocator::new(size.into()),
            format,
            raw,
            raw_view,
            live_atlas_keys: 0,
        };

        self.initializations.push(atlas_texture.id);
        self.texture_slots.insert(atlas_texture.id, slot);

        if slot < texture_list.textures.len() {
            texture_list.textures[slot] = Some(atlas_texture);
            texture_list
                .textures
                .get_mut(slot)
                .unwrap()
                .as_mut()
                .unwrap()
        } else {
            texture_list.textures.push(Some(atlas_texture));
            texture_list.textures.last_mut().unwrap().as_mut().unwrap()
        }
    }

    fn upload_texture(&mut self, id: AtlasTextureId, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        let data = self.upload_belt.alloc_bytes(bytes, &self.gpu);
        self.uploads.push(PendingUpload { id, bounds, data });
    }

    fn flush_initializations(&mut self, encoder: &mut gpu::CommandEncoder) {
        for id in std::mem::take(&mut self.initializations) {
            if let Some(texture) = self.texture(id) {
                encoder.init_texture(texture.raw);
            }
        }
    }

    fn flush(&mut self, encoder: &mut gpu::CommandEncoder) {
        self.flush_initializations(encoder);

        let mut transfers = encoder.transfer("atlas");
        for upload in std::mem::take(&mut self.uploads) {
            let Some(texture) = self.texture(upload.id) else {
                continue;
            };
            transfers.copy_buffer_to_texture(
                upload.data,
                upload.bounds.size.width.to_bytes(texture.bytes_per_pixel()),
                gpu::TexturePiece {
                    texture: texture.raw,
                    mip_level: 0,
                    array_layer: 0,
                    origin: [
                        upload.bounds.origin.x.into(),
                        upload.bounds.origin.y.into(),
                        0,
                    ],
                },
                gpu::Extent {
                    width: upload.bounds.size.width.into(),
                    height: upload.bounds.size.height.into(),
                    depth: 1,
                },
            );
        }
    }
}

#[derive(Default)]
struct BladeAtlasStorage {
    monochrome_textures: AtlasTextureList<BladeAtlasTexture>,
    polychrome_textures: AtlasTextureList<BladeAtlasTexture>,
}

impl ops::Index<AtlasTextureKind> for BladeAtlasStorage {
    type Output = AtlasTextureList<BladeAtlasTexture>;
    fn index(&self, kind: AtlasTextureKind) -> &Self::Output {
        match kind {
            crate::AtlasTextureKind::Monochrome => &self.monochrome_textures,
            crate::AtlasTextureKind::Polychrome => &self.polychrome_textures,
        }
    }
}

impl ops::IndexMut<AtlasTextureKind> for BladeAtlasStorage {
    fn index_mut(&mut self, kind: AtlasTextureKind) -> &mut Self::Output {
        match kind {
            crate::AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            crate::AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
        }
    }
}

impl BladeAtlasStorage {
    fn destroy(&mut self, gpu: &gpu::Context) {
        for mut texture in self.monochrome_textures.drain().flatten() {
            texture.destroy(gpu);
        }
        for mut texture in self.polychrome_textures.drain().flatten() {
            texture.destroy(gpu);
        }
    }
}

struct BladeAtlasTexture {
    id: AtlasTextureId,
    allocator: BucketedAtlasAllocator,
    raw: gpu::Texture,
    raw_view: gpu::TextureView,
    format: gpu::TextureFormat,
    live_atlas_keys: u32,
}

impl BladeAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(size.into())?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            padding: 0,
            bounds: Bounds {
                origin: allocation.rectangle.min.into(),
                size,
            },
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn destroy(&mut self, gpu: &gpu::Context) {
        gpu.destroy_texture(self.raw);
        gpu.destroy_texture_view(self.raw_view);
    }

    fn bytes_per_pixel(&self) -> u8 {
        self.format.block_info().size
    }

    /// Give a tile's rectangle back and drop the key that held it.
    fn deallocate(&mut self, tile: crate::platform::TileId) {
        self.allocator.deallocate(tile.into());
        self.live_atlas_keys = self.live_atlas_keys.saturating_sub(1);
    }

    fn is_unreferenced(&mut self) -> bool {
        self.live_atlas_keys == 0
    }
}

impl From<Size<DevicePixels>> for etagere::Size {
    fn from(size: Size<DevicePixels>) -> Self {
        etagere::Size::new(size.width.into(), size.height.into())
    }
}

impl From<etagere::Point> for Point<DevicePixels> {
    fn from(value: etagere::Point) -> Self {
        Point {
            x: DevicePixels::from(value.x),
            y: DevicePixels::from(value.y),
        }
    }
}

impl From<etagere::Size> for Size<DevicePixels> {
    fn from(size: etagere::Size) -> Self {
        Size {
            width: DevicePixels::from(size.width),
            height: DevicePixels::from(size.height),
        }
    }
}

impl From<etagere::Rectangle> for Bounds<DevicePixels> {
    fn from(rectangle: etagere::Rectangle) -> Self {
        Bounds {
            origin: rectangle.min.into(),
            size: rectangle.size().into(),
        }
    }
}
