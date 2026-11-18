pub mod cache;
pub mod canvas;
pub mod document;
pub mod image_view;
pub mod pages;
pub mod pdf;
pub mod pdf_view;
pub mod raster;

pub use image_view::{ImageEvent, ImageView};
pub use pdf_view::{PdfEvent, PdfView};

type Area = std::rc::Rc<std::cell::Cell<(gpui::Point<gpui::Pixels>, (f32, f32))>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomAction {
    In,
    Out,
    Fit,
    FitWidth,
    Percent(f32),
}

pub fn zoom_label(percent: i64) -> String {
    focus_i18n::t_args("reading-zoom-level", &[("percent", percent.into())])
}
