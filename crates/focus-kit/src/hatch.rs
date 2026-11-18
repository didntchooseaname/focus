use std::sync::Arc;

use gpui::{
    App, Bounds, ContentMask, Corners, Global, Hsla, IntoElement, Pixels, RenderImage, Rgba,
    Styled, Window, canvas, point, px, size,
};
use image::{Frame, RgbaImage};

use crate::tokens::{ActiveTokens, size::HATCH_PITCH};

const TILE_MIN: u32 = 448;

struct HatchTile {
    color: Hsla,
    scale: f32,
    image: Arc<RenderImage>,
}

impl Global for HatchTile {}

fn draw_tile(color: Hsla, scale: f32) -> (Arc<RenderImage>, u32) {
    let pitch = ((HATCH_PITCH as f32 * scale).round() as u32).max(2);
    let side = TILE_MIN.div_ceil(pitch) * pitch;
    let Rgba { r, g, b, a } = color.to_rgb();
    let byte = |v: f32| (v * 255.).round() as u8;
    let lit = image::Rgba([byte(b), byte(g), byte(r), byte(a)]);
    let mut tile = RgbaImage::new(side, side);
    for y in 0..side {
        for x in 0..side {
            if (x + y) % pitch == 0 {
                tile.put_pixel(x, y, lit);
            }
        }
    }
    (Arc::new(RenderImage::new(vec![Frame::new(tile)])), side)
}

fn tile(window: &mut Window, cx: &mut App) -> Arc<RenderImage> {
    let color = cx.tokens().hatch;
    let scale = window.scale_factor();
    if let Some(cached) = cx.try_global::<HatchTile>()
        && cached.color == color
        && cached.scale == scale
    {
        return cached.image.clone();
    }
    let (image, _) = draw_tile(color, scale);
    if let Some(old) = cx.try_global::<HatchTile>().map(|tile| tile.image.clone()) {
        cx.drop_image(old, Some(window));
    }
    cx.set_global(HatchTile {
        color,
        scale,
        image: image.clone(),
    });
    image
}

fn paint(bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let image = tile(window, cx);
    let side = px(image.size(0).width.0 as f32 / window.scale_factor());
    let first_x = side * (bounds.left() / side).floor();
    let first_y = side * (bounds.top() / side).floor();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut y = first_y;
        while y < bounds.bottom() {
            let mut x = first_x;
            while x < bounds.right() {
                let tile_bounds = Bounds {
                    origin: point(x, y),
                    size: size(side, side),
                };
                if window
                    .paint_image(tile_bounds, Corners::default(), image.clone(), 0, false)
                    .is_err()
                {
                    return;
                }
                x += side;
            }
            y += side;
        }
    });
}

pub fn hatch() -> impl IntoElement + Styled {
    canvas(
        |_, _, _| {},
        |bounds, _, window, cx| paint(bounds, window, cx),
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::draw_tile;
    use gpui::Hsla;

    #[test]
    fn tile_side_is_a_multiple_of_the_pitch() {
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let (image, side) = draw_tile(Hsla::white(), scale);
            let pitch = (12.0 * scale).round() as u32;
            assert_eq!(side % pitch, 0, "scale {scale}");
            assert_eq!(image.size(0).width.0 as u32, side);
        }
    }
}
