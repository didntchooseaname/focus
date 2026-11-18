use std::ops::Range;

pub const GAP: f32 = 12.;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Fixed(f32),
}

pub struct Layout {
    sizes: Vec<(f32, f32)>,
    tops: Vec<f32>,
    pub zoom: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub page: usize,
    pub x: f32,
    pub y: f32,
}

impl Zoom {
    pub fn resolve(self, sizes: &[(f32, f32)], area: (f32, f32), page: usize) -> f32 {
        let zoom = match self {
            Zoom::Fixed(zoom) => zoom,
            Zoom::FitWidth => {
                let widest = sizes.iter().map(|s| s.0).fold(1., f32::max);
                (area.0 - GAP * 2.).max(1.) / widest
            }
            Zoom::FitPage => {
                let (w, h) = sizes.get(page).copied().unwrap_or((1., 1.));
                ((area.0 - GAP * 2.).max(1.) / w.max(1.))
                    .min((area.1 - GAP * 2.).max(1.) / h.max(1.))
            }
        };
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }
}

impl Layout {
    pub fn new(sizes: &[(f32, f32)], zoom: f32) -> Layout {
        let mut tops = Vec::with_capacity(sizes.len());
        let mut y = GAP;
        let mut widest: f32 = 0.;
        for (w, h) in sizes {
            tops.push(y);
            y += h * zoom + GAP;
            widest = widest.max(w * zoom);
        }
        Layout {
            sizes: sizes.to_vec(),
            tops,
            zoom,
            width: widest + GAP * 2.,
            height: y,
        }
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    pub fn left(&self, area_width: f32) -> f32 {
        ((area_width - self.width) / 2.).max(0.)
    }

    pub fn rect(&self, page: usize) -> (f32, f32, f32, f32) {
        let (w, h) = self.sizes[page];
        let (w, h) = (w * self.zoom, h * self.zoom);
        ((self.width - w) / 2., self.tops[page], w, h)
    }

    pub fn visible(&self, top: f32, height: f32) -> Range<usize> {
        if self.is_empty() {
            return 0..0;
        }
        let first = self.page_at(top);
        let bottom = top + height;
        let mut end = first;
        while end < self.len() && self.tops[end] < bottom {
            end += 1;
        }
        first..end.max(first + 1).min(self.len())
    }

    pub fn page_at(&self, y: f32) -> usize {
        self.tops.partition_point(|top| *top <= y).saturating_sub(1)
    }

    pub fn top(&self, page: usize) -> f32 {
        self.tops.get(page).copied().unwrap_or(0.) - GAP
    }

    pub fn anchor(&self, at: (f32, f32)) -> Option<Anchor> {
        if self.is_empty() {
            return None;
        }
        let page = self.page_at(at.1);
        let (x, y, _, _) = self.rect(page);
        let (w, h) = self.sizes[page];
        Some(Anchor {
            page,
            x: ((at.0 - x) / self.zoom).clamp(0., w),
            y: ((at.1 - y) / self.zoom).clamp(0., h),
        })
    }

    pub fn point(&self, anchor: Anchor) -> (f32, f32) {
        let (x, y, _, _) = self.rect(anchor.page.min(self.len().saturating_sub(1)));
        (x + anchor.x * self.zoom, y + anchor.y * self.zoom)
    }
}

pub fn scroll_keeping(
    before: &Layout,
    after: &Layout,
    scroll: (f32, f32),
    pointer: (f32, f32),
    area: (f32, f32),
) -> (f32, f32) {
    let at = (
        scroll.0 + pointer.0 - before.left(area.0),
        scroll.1 + pointer.1,
    );
    let Some(anchor) = before.anchor(at) else {
        return (0., 0.);
    };
    let (x, y) = after.point(anchor);
    clamp_scroll(
        after,
        (x + after.left(area.0) - pointer.0, y - pointer.1),
        area,
    )
}

pub fn clamp_scroll(layout: &Layout, scroll: (f32, f32), area: (f32, f32)) -> (f32, f32) {
    (
        scroll.0.clamp(0., (layout.width - area.0).max(0.)),
        scroll.1.clamp(0., (layout.height - area.1).max(0.)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595., 842.);

    #[test]
    fn pages_are_stacked_twelve_pixels_apart() {
        let layout = Layout::new(&[A4, A4, (842., 595.)], 1.);
        assert_eq!(layout.rect(0), ((842. - 595.) / 2. + GAP, GAP, 595., 842.));
        assert_eq!(layout.rect(1).1, GAP + 842. + GAP);
        assert_eq!(layout.rect(2), (GAP, GAP * 3. + 842. * 2., 842., 595.));
        assert_eq!(layout.height, GAP * 4. + 842. * 2. + 595.);
        assert_eq!(layout.width, 842. + GAP * 2.);
    }

    #[test]
    fn fit_width_and_fit_page_follow_the_area() {
        let sizes = [A4, A4];
        assert_eq!(Zoom::FitWidth.resolve(&sizes, (619., 500.), 0), 1.);
        let fit = Zoom::FitPage.resolve(&sizes, (2000., 866.), 0);
        assert_eq!(fit, 1.);
        assert_eq!(Zoom::Fixed(100.).resolve(&sizes, (1., 1.), 0), MAX_ZOOM);
    }

    #[test]
    fn only_the_pages_in_view_are_listed() {
        let layout = Layout::new(&[A4; 100], 1.);
        let page = 842. + GAP;
        assert_eq!(layout.visible(0., 800.), 0..1);
        assert_eq!(layout.visible(page * 10. + GAP + 5., 900.), 10..12);
        assert_eq!(layout.visible(page * 10. + 5., 5.), 9..10);
        assert_eq!(layout.visible(layout.height - 10., 900.), 99..100);
        assert_eq!(layout.page_at(page * 3. + GAP + 1.), 3);
        assert_eq!(layout.page_at(page * 4. + 6.), 3);
    }

    #[test]
    fn a_zoom_keeps_the_page_point_under_the_pointer() {
        let sizes = [A4; 10];
        let area = (1000., 800.);
        let before = Layout::new(&sizes, 1.);
        let scroll = (0., 2000.);
        let pointer = (400., 300.);
        let after = Layout::new(&sizes, 2.);
        let next = scroll_keeping(&before, &after, scroll, pointer, area);
        let anchor_before = before
            .anchor((
                scroll.0 + pointer.0 - before.left(area.0),
                scroll.1 + pointer.1,
            ))
            .unwrap();
        let anchor_after = after
            .anchor((next.0 + pointer.0 - after.left(area.0), next.1 + pointer.1))
            .unwrap();
        assert_eq!(anchor_before.page, anchor_after.page);
        assert!((anchor_before.x - anchor_after.x).abs() < 0.01);
        assert!((anchor_before.y - anchor_after.y).abs() < 0.01);
    }

    #[test]
    fn the_scroll_stays_inside_the_pages() {
        let layout = Layout::new(&[A4; 2], 1.);
        assert_eq!(
            clamp_scroll(&layout, (-50., 99999.), (2000., 800.)),
            (0., layout.height - 800.)
        );
    }
}
