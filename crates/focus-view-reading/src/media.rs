use focus_kit::Tokens;
use gpui::{App, AppContext, Context, Entity, Global, RenderImage};
use image::{Frame, RgbaImage};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use super::model::ImageSource;

const BUDGET: usize = 96 * 1024 * 1024;
const MAX_SIDE: u32 = 1520;
const MAX_DECODED_SIDE: u32 = 20_000;
const MAX_DECODED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone)]
pub enum Slot {
    Loading,
    Ready {
        image: Arc<RenderImage>,
        width: f32,
        height: f32,
        label: String,
        links: Arc<Vec<focus_md::mermaid::DiagramLink>>,
    },
    Failed(String),
}

struct Entry {
    slot: Slot,
    bytes: usize,
}

pub struct Media {
    entries: HashMap<u64, Entry>,
    order: VecDeque<u64>,
    bytes: usize,
}

struct MediaGlobal(Entity<Media>);
impl Global for MediaGlobal {}

pub fn media(cx: &mut App) -> Entity<Media> {
    if let Some(global) = cx.try_global::<MediaGlobal>() {
        return global.0.clone();
    }
    let entity = cx.new(|_| Media {
        entries: HashMap::new(),
        order: VecDeque::new(),
        bytes: 0,
    });
    cx.set_global(MediaGlobal(entity.clone()));
    entity
}

pub fn loading(cx: &App) -> usize {
    focus_view_media::document::pending(cx)
        + cx.try_global::<MediaGlobal>().map_or(0, |global| {
            global
                .0
                .read(cx)
                .entries
                .values()
                .filter(|e| matches!(e.slot, Slot::Loading))
                .count()
        })
}

fn key_of(parts: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

fn missing(src: &str) -> String {
    focus_i18n::t_args("reading-image-missing", &[("src", src.into())])
}

fn bgra(mut rgba: RgbaImage) -> Arc<RenderImage> {
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(rgba)]))
}

fn decode(bytes: &[u8]) -> Result<(RgbaImage, f32, f32), String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let (w, h) = (decoded.width(), decoded.height());
    let decoded = if w.max(h) > MAX_SIDE {
        decoded.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        decoded
    };
    Ok((decoded.to_rgba8(), w as f32, h as f32))
}

fn fetch(source: &ImageSource) -> Result<Vec<u8>, String> {
    match source {
        ImageSource::File(path) => {
            std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
        }
        ImageSource::Url(url) => crate::remote::fetch(url).map_err(|refused| refused.text(url)),
        ImageSource::Missing(src) => Err(missing(src)),
        ImageSource::Pdf(embed) => Err(missing(&embed.target)),
    }
}

impl Media {
    fn touch(&mut self, key: u64) {
        if let Some(at) = self.order.iter().position(|k| *k == key) {
            self.order.remove(at);
        }
        self.order.push_back(key);
    }

    fn store(&mut self, key: u64, slot: Slot, bytes: usize, cx: &mut Context<Self>) {
        self.bytes += bytes;
        self.entries.insert(key, Entry { slot, bytes });
        self.touch(key);
        while self.bytes > BUDGET && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes -= entry.bytes;
                if let Slot::Ready { image, .. } = entry.slot {
                    cx.drop_image(image, None);
                }
            }
        }
        cx.notify();
    }

    pub fn image(&mut self, source: &ImageSource, cx: &mut Context<Self>) -> Slot {
        let key = match source {
            ImageSource::File(path) => key_of(("file", path)),
            ImageSource::Url(url) => key_of(("url", url)),
            ImageSource::Missing(src) => return Slot::Failed(missing(src)),
            ImageSource::Pdf(embed) => return Slot::Failed(missing(&embed.target)),
        };
        if let Some(entry) = self.entries.get(&key) {
            let slot = entry.slot.clone();
            self.touch(key);
            return slot;
        }
        self.entries.insert(
            key,
            Entry {
                slot: Slot::Loading,
                bytes: 0,
            },
        );
        let source = source.clone();
        let task =
            cx.background_spawn(async move { fetch(&source).and_then(|bytes| decode(&bytes)) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.entries.remove(&key);
                match result {
                    Ok((rgba, width, height)) => {
                        let bytes = rgba.len();
                        this.store(
                            key,
                            Slot::Ready {
                                image: bgra(rgba),
                                width,
                                height,
                                label: String::new(),
                                links: Arc::new(Vec::new()),
                            },
                            bytes,
                            cx,
                        );
                    }
                    Err(message) => this.store(key, Slot::Failed(message), 0, cx),
                }
            });
        })
        .detach();
        Slot::Loading
    }

    pub fn mermaid(
        &mut self,
        source: &str,
        tokens: &Tokens,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Slot {
        let theme = mermaid_theme(tokens);
        let key = key_of((
            "mermaid",
            source,
            &theme.background,
            &theme.text,
            &theme.edge,
            (scale * 100.).round() as u32,
        ));
        if let Some(entry) = self.entries.get(&key) {
            let slot = entry.slot.clone();
            self.touch(key);
            return slot;
        }
        self.entries.insert(
            key,
            Entry {
                slot: Slot::Loading,
                bytes: 0,
            },
        );
        let source = source.to_string();
        let task = cx.background_spawn(async move {
            let svg = focus_md::mermaid::render_svg(&source, &theme)?;
            let raster = focus_md::mermaid::rasterize(&svg, scale, &[])?;
            let rgba =
                RgbaImage::from_raw(raster.width, raster.height, raster.rgba).ok_or_else(|| {
                    focus_md::mermaid::Error::Render("incomplete image buffer".into())
                })?;
            Ok::<_, focus_md::mermaid::Error>((rgba, svg.width, svg.height, svg.diagram, svg.links))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.entries.remove(&key);
                match result {
                    Ok((rgba, width, height, label, links)) => {
                        let bytes = rgba.len();
                        this.store(
                            key,
                            Slot::Ready {
                                image: bgra(rgba),
                                width,
                                height,
                                label,
                                links: Arc::new(links),
                            },
                            bytes,
                            cx,
                        );
                    }
                    Err(error) => this.store(key, Slot::Failed(diagram_error(&error)), 0, cx),
                }
            });
        })
        .detach();
        Slot::Loading
    }
}

fn diagram_error(error: &focus_md::mermaid::Error) -> String {
    use focus_md::mermaid::Error;
    let (key, detail) = match error {
        Error::Syntax(detail) => ("reading-diagram-syntax", detail),
        Error::Unsupported(kind) => ("reading-diagram-unsupported", kind),
        Error::Render(detail) => ("reading-diagram-failed", detail),
    };
    focus_i18n::t_args(key, &[("detail", focus_i18n::Arg::Str(detail))])
}

fn hex(color: gpui::Hsla) -> String {
    let c = color.to_rgb();
    let b = |v: f32| (v * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

pub fn mermaid_theme(t: &Tokens) -> focus_md::mermaid::Theme {
    focus_md::mermaid::Theme {
        background: hex(t.bg),
        node_fill: hex(t.card),
        node_stroke: hex(t.line_2),
        text: hex(t.ink),
        muted_text: hex(t.ink_2),
        edge: hex(t.ink_3),
        font_family: focus_kit::typography::SANS.into(),
        font_size: 14.,
    }
}
