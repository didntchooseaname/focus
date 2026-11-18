use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, AppContext as _, Context, Entity, Global, RenderImage, WeakEntity};
use image::{Frame, RgbaImage};

use crate::cache::Cache;
use crate::pdf::worker::{Answer, Client, Request};
use crate::pdf::{Link, OutlineItem, PageText, PdfError, Region};

pub const CACHE_BYTES: usize = 96 << 20;

pub type TileKey = (usize, u32, [u32; 4]);

pub fn whole_tile(page: usize, size: (f32, f32), scale: f32) -> Option<(TileKey, Region)> {
    let region = Region::whole(size, scale)?;
    Some((
        (page, scale_key(scale), [0, 0, region.width, region.height]),
        region,
    ))
}

pub fn scale_key(scale: f32) -> u32 {
    (scale * 1000.).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wanted {
    Render(TileKey, Region),
    Text(usize),
    Links(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Load {
    Opening,
    Failed(PdfError),
    Ready,
}

pub struct PdfDocument {
    load: Load,
    client: Option<Client>,
    sizes: Vec<(f32, f32)>,
    tiles: Cache<TileKey, Arc<RenderImage>>,
    asked: Option<Wanted>,
    sent: usize,
    queue: VecDeque<Wanted>,
    failed: HashSet<usize>,
    texts: HashMap<usize, Arc<PageText>>,
    links: HashMap<usize, Arc<Vec<Link>>>,
    outline: Option<Arc<Vec<OutlineItem>>>,
}

#[derive(Default)]
struct Documents(HashMap<PathBuf, WeakEntity<PdfDocument>>);

impl Global for Documents {}

pub fn open(path: &Path, cx: &mut App) -> Entity<PdfDocument> {
    if let Some(open) = cx
        .try_global::<Documents>()
        .and_then(|d| d.0.get(path))
        .and_then(WeakEntity::upgrade)
    {
        return open;
    }
    let document = cx.new(|cx| PdfDocument::new(path.to_path_buf(), cx));
    let documents = cx.default_global::<Documents>();
    documents.0.retain(|_, weak| weak.upgrade().is_some());
    documents.0.insert(path.to_path_buf(), document.downgrade());
    document
}

pub fn pending(cx: &App) -> usize {
    cx.try_global::<Documents>().map_or(0, |documents| {
        documents
            .0
            .values()
            .filter_map(WeakEntity::upgrade)
            .filter(|d| {
                let d = d.read(cx);
                d.load == Load::Opening || d.asked.is_some() || !d.queue.is_empty()
            })
            .count()
    })
}

impl PdfDocument {
    fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        cx.on_release(|document: &mut PdfDocument, cx: &mut App| {
            for image in document.tiles.remove(|_| true) {
                cx.drop_image(image, None);
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let started = cx
                .background_spawn(async move { Client::spawn(&path) })
                .await;
            let opened = match started {
                Ok((client, opened)) => {
                    let answer = opened.recv().await.unwrap_or(Err(PdfError::WorkerStopped));
                    Some((client, answer))
                }
                Err(error) => {
                    eprintln!("focus: starting the PDF worker: {error}");
                    None
                }
            };
            let _ = this.update(cx, |document, cx| document.opened(opened, cx));
        })
        .detach();
        PdfDocument {
            load: Load::Opening,
            client: None,
            sizes: Vec::new(),
            tiles: Cache::new(CACHE_BYTES),
            asked: None,
            sent: 0,
            queue: VecDeque::new(),
            failed: HashSet::new(),
            texts: HashMap::new(),
            links: HashMap::new(),
            outline: None,
        }
    }

    fn opened(
        &mut self,
        opened: Option<(Client, Result<Answer, PdfError>)>,
        cx: &mut Context<Self>,
    ) {
        match opened {
            Some((client, Ok(Answer::Opened { sizes }))) if !sizes.is_empty() => {
                self.sizes = sizes;
                self.load = Load::Ready;
                let outline = client.send(Request::Outline);
                self.client = Some(client);
                cx.spawn(async move |this, cx| {
                    let answer = outline.recv().await;
                    let _ = this.update(cx, |document, cx| {
                        if let Ok(Ok(Answer::Outline(items))) = answer {
                            document.outline = Some(Arc::new(items));
                        }
                        cx.notify();
                    });
                })
                .detach();
                self.pump(cx);
            }
            Some((_, Err(error))) => self.load = Load::Failed(error),
            _ => self.load = Load::Failed(PdfError::WorkerStopped),
        }
        cx.notify();
    }

    pub fn load(&self) -> Load {
        self.load
    }

    pub fn sizes(&self) -> &[(f32, f32)] {
        &self.sizes
    }

    pub fn outline(&self) -> Option<Arc<Vec<OutlineItem>>> {
        self.outline.clone()
    }

    pub fn text(&self, page: usize) -> Option<&Arc<PageText>> {
        self.texts.get(&page)
    }

    pub fn texts(&self) -> impl Iterator<Item = (&usize, &Arc<PageText>)> {
        self.texts.iter()
    }

    pub fn links(&self, page: usize) -> Option<&Arc<Vec<Link>>> {
        self.links.get(&page)
    }

    pub fn failed(&self, page: usize) -> bool {
        self.failed.contains(&page)
    }

    pub fn has_tile(&self, key: &TileKey) -> bool {
        self.tiles.contains(key)
    }

    pub fn tile(&mut self, key: &TileKey) -> Option<Arc<RenderImage>> {
        self.tiles.get(key)
    }

    pub fn tiles(&self, keep: impl Fn(&TileKey) -> bool) -> Vec<(TileKey, Arc<RenderImage>)> {
        self.tiles
            .find(keep)
            .into_iter()
            .map(|(k, v)| (*k, v))
            .collect()
    }

    pub fn cached_bytes(&self) -> usize {
        self.tiles.bytes()
    }

    pub fn asked_total(&self) -> usize {
        self.sent
    }

    pub fn idle(&self) -> bool {
        self.asked.is_none() && self.queue.is_empty()
    }

    fn done(&self, wanted: &Wanted) -> bool {
        match wanted {
            Wanted::Render((page, _, _), _) if self.failed.contains(page) => true,
            Wanted::Render(key, _) => self.tiles.contains(key),
            Wanted::Text(page) => self.texts.contains_key(page),
            Wanted::Links(page) => self.links.contains_key(page),
        }
    }

    pub fn ask(&mut self, wanted: Wanted, cx: &mut Context<Self>) {
        if self.done(&wanted) || self.asked == Some(wanted) || self.queue.contains(&wanted) {
            return;
        }
        self.queue.push_back(wanted);
        self.pump(cx);
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.asked.is_some() || self.load != Load::Ready {
            return;
        }
        let wanted = loop {
            let Some(wanted) = self.queue.pop_front() else {
                return;
            };
            if !self.done(&wanted) {
                break wanted;
            }
        };
        let Some(client) = &self.client else { return };
        let receiver = client.send(match wanted {
            Wanted::Render((page, _, _), region) => Request::Render { page, region },
            Wanted::Text(page) => Request::Text { page },
            Wanted::Links(page) => Request::Links { page },
        });
        self.asked = Some(wanted);
        self.sent += 1;
        cx.spawn(async move |this, cx| {
            let answer = receiver
                .recv()
                .await
                .unwrap_or(Err(PdfError::WorkerStopped));
            let _ = this.update(cx, |document, cx| document.answered(wanted, answer, cx));
        })
        .detach();
    }

    fn answered(
        &mut self,
        wanted: Wanted,
        answer: Result<Answer, PdfError>,
        cx: &mut Context<Self>,
    ) {
        self.asked = None;
        match (wanted, answer) {
            (_, Err(PdfError::WorkerStopped)) => {
                self.load = Load::Failed(PdfError::WorkerStopped);
                self.client = None;
                self.queue.clear();
            }
            (Wanted::Render(key, _), Ok(Answer::Bitmap(bitmap))) => {
                let bytes = bitmap.bgra.len();
                if let Some(pixels) = RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.bgra)
                {
                    let image = Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
                    for old in self.tiles.insert(key, image, bytes) {
                        cx.drop_image(old, None);
                    }
                }
            }
            (Wanted::Render((page, _, _), _), _) => {
                self.failed.insert(page);
            }
            (Wanted::Text(page), answer) => {
                let text = match answer {
                    Ok(Answer::Text(text)) => text,
                    _ => PageText::default(),
                };
                self.texts.insert(page, Arc::new(text));
            }
            (Wanted::Links(page), answer) => {
                let links = match answer {
                    Ok(Answer::Links(links)) => links,
                    _ => Vec::new(),
                };
                self.links.insert(page, Arc::new(links));
            }
        }
        self.pump(cx);
        cx.notify();
    }
}
