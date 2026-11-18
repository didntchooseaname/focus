pub mod nav;
#[cfg(test)]
pub(crate) mod sample;
pub mod text;
pub mod worker;

use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Page;
use hayro::{RenderCache, RenderSettings};
use serde::{Deserialize, Serialize};

pub use nav::{Link, LinkTarget, OutlineItem};
pub use text::PageText;

pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_BITMAP_SIDE: u32 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfError {
    Protected,
    UnsupportedEncryption,
    Unreadable,
    TooLarge,
    WorkerStopped,
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PdfError::Protected => "the document is protected by a password",
            PdfError::UnsupportedEncryption => "the document uses an unsupported encryption",
            PdfError::Unreadable => "the document cannot be read",
            PdfError::TooLarge => "the document is too large",
            PdfError::WorkerStopped => "the PDF worker stopped",
        })
    }
}

impl std::error::Error for PdfError {}

pub fn open(bytes: Vec<u8>) -> Result<Pdf, PdfError> {
    use hayro::hayro_syntax::{DecryptionError, LoadPdfError};
    Pdf::new(Arc::new(bytes)).map_err(|error| match error {
        LoadPdfError::Decryption(DecryptionError::PasswordProtected) => PdfError::Protected,
        LoadPdfError::Decryption(DecryptionError::UnsupportedAlgorithm) => {
            PdfError::UnsupportedEncryption
        }
        LoadPdfError::Decryption(_) | LoadPdfError::Invalid => PdfError::Unreadable,
    })
}

pub fn page_sizes(pdf: &Pdf) -> Vec<(f32, f32)> {
    pdf.pages().iter().map(Page::render_dimensions).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub scale: f32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub fn whole(size: (f32, f32), scale: f32) -> Option<Region> {
        let (width, height) = (
            (size.0 * scale).ceil().max(1.) as u32,
            (size.1 * scale).ceil().max(1.) as u32,
        );
        (width.max(height) <= MAX_BITMAP_SIDE).then_some(Region {
            scale,
            x: 0,
            y: 0,
            width,
            height,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

pub struct Renderer<'a> {
    cache: RenderCache<'a>,
    settings: InterpreterSettings,
}

impl<'a> Renderer<'a> {
    pub fn new() -> Self {
        Renderer {
            cache: RenderCache::new(),
            settings: InterpreterSettings::default(),
        }
    }

    pub fn render(&self, page: &'a Page<'a>, region: Region) -> Bitmap {
        use hayro::hayro_interpret::TransformExt as _;
        use hayro::kurbo::Affine;
        use hayro::vello_cpu::{self, Pixmap, RenderContext};

        let width = region.width.clamp(1, MAX_BITMAP_SIDE) as u16;
        let height = region.height.clamp(1, MAX_BITMAP_SIDE) as u16;
        let mut ctx = RenderContext::new(width, height);
        let scale = f64::from(region.scale);
        let transform = Affine::translate((-f64::from(region.x), -f64::from(region.y)))
            * Affine::scale(scale)
            * page.initial_transform(true).to_kurbo();
        hayro::render_into(
            page,
            &self.cache,
            &self.settings,
            &RenderSettings::default(),
            &mut ctx,
            transform,
        );
        ctx.flush();
        let mut pixmap = Pixmap::new(width, height);
        ctx.render_with(
            &mut pixmap,
            &mut vello_cpu::Resources::default(),
            vello_cpu::RasterizerSettings {
                target_init: vello_cpu::TargetInit::Clear(vello_cpu::color::palette::css::WHITE),
                ..Default::default()
            },
        );
        let mut bgra = pixmap.data_as_u8_slice().to_vec();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Bitmap {
            width: u32::from(width),
            height: u32::from(height),
            bgra,
        }
    }

    pub fn text(&self, page: &'a Page<'a>) -> PageText {
        text::extract(page, &self.settings)
    }
}

impl Default for Renderer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn page_anchor(anchor: &str) -> Option<usize> {
    let value = anchor.trim().strip_prefix("page=")?;
    let page: usize = value.trim().parse().ok()?;
    page.checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_counts_and_sizes_are_read_without_drawing() {
        let pdf = open(sample::document()).unwrap();
        assert_eq!(
            page_sizes(&pdf),
            vec![(595., 842.), (595., 842.), (842., 595.)]
        );
    }

    #[test]
    fn a_page_is_drawn_on_white_paper_at_the_scale_asked() {
        let pdf = open(sample::document()).unwrap();
        let renderer = Renderer::new();
        let pages = pdf.pages();
        let region = Region::whole((595., 842.), 0.5).unwrap();
        let bitmap = renderer.render(&pages[0], region);
        assert_eq!((bitmap.width, bitmap.height), (298, 421));
        assert_eq!(&bitmap.bgra[..4], &[255, 255, 255, 255]);
        let at = |x: u32, y: u32| {
            let i = ((y * bitmap.width + x) * 4) as usize;
            bitmap.bgra[i..i + 4].to_vec()
        };
        assert_eq!(at(60, 41), vec![0, 0, 0, 255]);
        let part = renderer.render(
            &pages[0],
            Region {
                scale: 0.5,
                x: 60,
                y: 36,
                width: 40,
                height: 10,
            },
        );
        assert_eq!((part.width, part.height), (40, 10));
        assert_eq!(
            &part.bgra[(5 * 40 + 5) * 4..(5 * 40 + 5) * 4 + 4],
            &[0, 0, 0, 255]
        );
    }

    #[test]
    fn a_page_too_large_for_one_bitmap_is_drawn_by_regions() {
        assert!(Region::whole((595., 842.), 9.).is_some());
        assert!(Region::whole((595., 842.), 10.).is_none());
    }

    #[test]
    fn a_protected_or_broken_document_is_refused_without_a_panic() {
        assert_eq!(open(sample::protected()).err(), Some(PdfError::Protected));
        assert_eq!(
            open(b"%PDF-1.7\n garbage".to_vec()).err(),
            Some(PdfError::Unreadable)
        );
        assert_eq!(open(Vec::new()).err(), Some(PdfError::Unreadable));
        let mut cut = sample::document();
        cut.truncate(cut.len() / 3);
        if let Ok(pdf) = open(cut) {
            let renderer = Renderer::new();
            for page in pdf.pages().iter() {
                let size = page.render_dimensions();
                renderer.render(page, Region::whole(size, 0.2).unwrap());
                renderer.text(page);
            }
        }
    }

    #[test]
    fn page_anchors_are_read_from_one() {
        assert_eq!(page_anchor("page=3"), Some(2));
        assert_eq!(page_anchor("page=1"), Some(0));
        assert_eq!(page_anchor("page=0"), None);
        assert_eq!(page_anchor("page=x"), None);
        assert_eq!(page_anchor("Heading"), None);
    }
}
