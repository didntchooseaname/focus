use std::path::Path;
use std::sync::{Arc, OnceLock};

use image::{DynamicImage, ImageReader, RgbaImage};

pub const IMAGE_EXTENSIONS: [&str; 21] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "ico", "tga", "qoi", "pbm", "pgm",
    "ppm", "pam", "hdr", "exr", "dds", "ff", "farbfeld", "svg",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Svg,
    Pdf,
}

impl Kind {
    pub fn of(path: &Path) -> Option<Kind> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "pdf" => Some(Kind::Pdf),
            "svg" => Some(Kind::Svg),
            e if IMAGE_EXTENSIONS.contains(&e) => Some(Kind::Image),
            _ => None,
        }
    }
}

const MAX_SIDE: u32 = 30_000;
const MAX_ALLOC: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Unreadable(String),
    TooLarge { width: u32, height: u32 },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Unreadable(reason) => write!(f, "unreadable image: {reason}"),
            DecodeError::TooLarge { width, height } => {
                write!(f, "image too large to decode: {width}x{height}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

pub struct Picture {
    pub bgra: RgbaImage,
    pub full_width: u32,
    pub full_height: u32,
}

impl Picture {
    pub fn width(&self) -> u32 {
        self.bgra.width()
    }

    pub fn height(&self) -> u32 {
        self.bgra.height()
    }

    pub fn rotated(&self, quarter_turns: u8) -> Picture {
        let bgra = match quarter_turns % 4 {
            1 => image::imageops::rotate90(&self.bgra),
            2 => image::imageops::rotate180(&self.bgra),
            3 => image::imageops::rotate270(&self.bgra),
            _ => self.bgra.clone(),
        };
        let (full_width, full_height) = if quarter_turns % 2 == 1 {
            (self.full_height, self.full_width)
        } else {
            (self.full_width, self.full_height)
        };
        Picture {
            bgra,
            full_width,
            full_height,
        }
    }
}

pub fn format_hint(path: &Path) -> Option<image::ImageFormat> {
    image::ImageFormat::from_path(path).ok()
}

fn reader(
    bytes: &[u8],
    hint: Option<image::ImageFormat>,
) -> Result<ImageReader<std::io::Cursor<&[u8]>>, DecodeError> {
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| DecodeError::Unreadable(e.to_string()))?;
    if reader.format().is_none()
        && let Some(hint) = hint
    {
        reader.set_format(hint);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    Ok(reader)
}

fn svg_tree(bytes: &[u8]) -> Result<resvg::usvg::Tree, DecodeError> {
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: resvg::usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..resvg::usvg::Options::default()
    };
    resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|e| DecodeError::Unreadable(e.to_string()))
}

fn fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

pub fn dimensions(
    bytes: &[u8],
    kind: Kind,
    hint: Option<image::ImageFormat>,
) -> Result<(u32, u32), DecodeError> {
    match kind {
        Kind::Svg => {
            let size = svg_tree(bytes)?.size();
            Ok((size.width().ceil() as u32, size.height().ceil() as u32))
        }
        _ => reader(bytes, hint)?
            .into_dimensions()
            .map_err(|e| DecodeError::Unreadable(e.to_string())),
    }
}

pub fn decode(
    bytes: &[u8],
    kind: Kind,
    hint: Option<image::ImageFormat>,
    longest: u32,
) -> Result<Picture, DecodeError> {
    let longest = longest.max(1);
    match kind {
        Kind::Svg => decode_svg(bytes, longest),
        _ => {
            let reader = reader(bytes, hint)?;
            let decoded = reader.decode().map_err(|e| match e {
                image::ImageError::Limits(_) => {
                    let (width, height) = dimensions(bytes, kind, hint).unwrap_or((0, 0));
                    DecodeError::TooLarge { width, height }
                }
                e => DecodeError::Unreadable(e.to_string()),
            })?;
            let (full_width, full_height) = (decoded.width(), decoded.height());
            let reduced = if full_width.max(full_height) > longest {
                decoded.thumbnail(longest, longest)
            } else {
                decoded
            };
            Ok(Picture {
                bgra: bgra(reduced),
                full_width,
                full_height,
            })
        }
    }
}

fn decode_svg(bytes: &[u8], longest: u32) -> Result<Picture, DecodeError> {
    let tree = svg_tree(bytes)?;
    let size = tree.size();
    let (full_width, full_height) = (size.width().ceil() as u32, size.height().ceil() as u32);
    if full_width.max(full_height) > MAX_SIDE {
        return Err(DecodeError::TooLarge {
            width: full_width,
            height: full_height,
        });
    }
    let scale = longest as f32 / size.width().max(size.height()).max(1.);
    let (width, height) = (
        ((size.width() * scale).ceil() as u32).max(1),
        ((size.height() * scale).ceil() as u32).max(1),
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or(DecodeError::TooLarge {
        width: full_width,
        height: full_height,
    })?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut out = RgbaImage::new(width, height);
    for (pixel, source) in out.pixels_mut().zip(pixmap.pixels()) {
        let c = source.demultiply();
        pixel.0 = [c.blue(), c.green(), c.red(), c.alpha()];
    }
    Ok(Picture {
        bgra: out,
        full_width,
        full_height,
    })
}

fn bgra(image: DynamicImage) -> RgbaImage {
    let mut rgba = image.into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    rgba
}

pub fn png_for_clipboard(
    bytes: &[u8],
    kind: Kind,
    hint: Option<image::ImageFormat>,
) -> Result<Vec<u8>, DecodeError> {
    if kind == Kind::Image && image::guess_format(bytes).ok() == Some(image::ImageFormat::Png) {
        return Ok(bytes.to_vec());
    }
    let longest = match kind {
        Kind::Svg => {
            let (w, h) = dimensions(bytes, kind, hint)?;
            w.max(h)
        }
        _ => MAX_SIDE,
    };
    let picture = decode(bytes, kind, hint, longest)?;
    let mut rgba = picture.bgra;
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| DecodeError::Unreadable(e.to_string()))?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgba};

    fn sample() -> DynamicImage {
        let mut image = RgbaImage::new(6, 4);
        for (x, _, pixel) in image.enumerate_pixels_mut() {
            *pixel = if x < 3 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            };
        }
        DynamicImage::ImageRgba8(image)
    }

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let image = match format {
            ImageFormat::Jpeg | ImageFormat::Pnm | ImageFormat::Tga => {
                DynamicImage::ImageRgb8(sample().to_rgb8())
            }
            ImageFormat::Hdr | ImageFormat::OpenExr => {
                DynamicImage::ImageRgb32F(sample().to_rgb32f())
            }
            ImageFormat::Farbfeld => DynamicImage::ImageRgba16(sample().to_rgba16()),
            _ => sample(),
        };
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap_or_else(|e| panic!("{format:?}: {e}"));
        out
    }

    fn dds() -> Vec<u8> {
        let mut out = b"DDS ".to_vec();
        let mut header = [0u32; 31];
        header[0] = 124;
        header[1] = 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000;
        header[2] = 4;
        header[3] = 4;
        header[4] = 8;
        header[18] = 32;
        header[19] = 0x4;
        header[20] = u32::from_le_bytes(*b"DXT1");
        header[26] = 0x1000;
        for value in header {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&[0x00, 0xf8, 0x00, 0xf8, 0, 0, 0, 0]);
        out
    }

    #[test]
    fn every_listed_raster_format_is_decoded() {
        let formats = [
            ("png", ImageFormat::Png),
            ("jpg", ImageFormat::Jpeg),
            ("gif", ImageFormat::Gif),
            ("webp", ImageFormat::WebP),
            ("bmp", ImageFormat::Bmp),
            ("tiff", ImageFormat::Tiff),
            ("ico", ImageFormat::Ico),
            ("tga", ImageFormat::Tga),
            ("qoi", ImageFormat::Qoi),
            ("ppm", ImageFormat::Pnm),
            ("hdr", ImageFormat::Hdr),
            ("exr", ImageFormat::OpenExr),
            ("ff", ImageFormat::Farbfeld),
        ];
        for (extension, format) in formats {
            assert_eq!(
                Kind::of(Path::new(&format!("a.{extension}"))),
                Some(Kind::Image)
            );
            let bytes = encoded(format);
            assert_eq!(
                dimensions(&bytes, Kind::Image, Some(format)),
                Ok((6, 4)),
                "{extension}"
            );
            let picture = decode(&bytes, Kind::Image, Some(format), 100)
                .unwrap_or_else(|e| panic!("{extension}: {e}"));
            assert_eq!((picture.width(), picture.height()), (6, 4), "{extension}");
            let left = picture.bgra.get_pixel(0, 0).0;
            assert!(left[2] > 200 && left[0] < 60, "{extension}: {left:?}");
        }
        let picture = decode(&dds(), Kind::Image, None, 100).unwrap();
        assert_eq!((picture.full_width, picture.full_height), (4, 4));
        assert!(picture.bgra.get_pixel(1, 1).0[2] > 200);
    }

    #[test]
    fn an_svg_is_drawn_at_the_size_asked_and_reads_no_other_file() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="40" height="20">
            <rect width="40" height="20" fill="#00ff00"/>
            <image href="/etc/hostname" width="40" height="20"/>
        </svg>"##;
        assert_eq!(dimensions(svg, Kind::Svg, None), Ok((40, 20)));
        let picture = decode(svg, Kind::Svg, None, 200).unwrap();
        assert_eq!((picture.width(), picture.height()), (200, 100));
        assert_eq!((picture.full_width, picture.full_height), (40, 20));
        assert_eq!(picture.bgra.get_pixel(100, 50).0, [0, 255, 0, 255]);
    }

    #[test]
    fn a_large_image_is_reduced_to_the_size_asked() {
        let big = DynamicImage::ImageRgba8(RgbaImage::new(3000, 1500));
        let mut bytes = Vec::new();
        big.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let picture = decode(&bytes, Kind::Image, None, 600).unwrap();
        assert_eq!((picture.width(), picture.height()), (600, 300));
        assert_eq!((picture.full_width, picture.full_height), (3000, 1500));
    }

    #[test]
    fn an_animated_gif_gives_its_first_frame() {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for colour in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                let frame = image::Frame::new(RgbaImage::from_pixel(2, 2, Rgba(colour)));
                encoder.encode_frame(frame).unwrap();
            }
        }
        let picture = decode(&bytes, Kind::Image, None, 10).unwrap();
        assert!(picture.bgra.get_pixel(0, 0).0[2] > 200);
    }

    #[test]
    fn a_declared_size_over_the_limit_is_refused_before_decoding() {
        let mut bytes = encoded(ImageFormat::Png);
        bytes[16..20].copy_from_slice(&40_000u32.to_be_bytes());
        assert!(matches!(
            decode(&bytes, Kind::Image, None, 100),
            Err(DecodeError::TooLarge { .. }) | Err(DecodeError::Unreadable(_))
        ));
        assert!(decode(b"not an image", Kind::Image, None, 100).is_err());
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides() {
        let picture = decode(&encoded(ImageFormat::Png), Kind::Image, None, 100).unwrap();
        let turned = picture.rotated(1);
        assert_eq!((turned.width(), turned.height()), (4, 6));
        assert_eq!((turned.full_width, turned.full_height), (4, 6));
        assert!(turned.bgra.get_pixel(0, 0).0[2] > 200);
        assert!(turned.bgra.get_pixel(0, 5).0[0] > 200);
        assert_eq!(picture.rotated(4).bgra, picture.bgra);
    }

    #[test]
    fn a_png_goes_to_the_clipboard_as_it_is() {
        let png = encoded(ImageFormat::Png);
        assert_eq!(png_for_clipboard(&png, Kind::Image, None).unwrap(), png);
        let from_bmp = png_for_clipboard(&encoded(ImageFormat::Bmp), Kind::Image, None).unwrap();
        assert_eq!(image::guess_format(&from_bmp).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn files_are_known_by_their_extension() {
        assert_eq!(Kind::of(Path::new("Docs/Report.PDF")), Some(Kind::Pdf));
        assert_eq!(Kind::of(Path::new("shot.Png")), Some(Kind::Image));
        assert_eq!(Kind::of(Path::new("logo.svg")), Some(Kind::Svg));
        assert_eq!(Kind::of(Path::new("photo.avif")), None);
        assert_eq!(Kind::of(Path::new("notes.md")), None);
        assert_eq!(Kind::of(Path::new("noext")), None);
    }
}
