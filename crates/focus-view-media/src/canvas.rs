pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 16.;
pub const STEP: f32 = 1.25;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Canvas {
    zoom: Option<f32>,
    offset: (f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Placement {
    pub fn contains(&self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

pub fn fit_zoom(area: (f32, f32), content: (f32, f32), margin: f32) -> f32 {
    let w = (area.0 - margin * 2.).max(1.) / content.0.max(1.);
    let h = (area.1 - margin * 2.).max(1.) / content.1.max(1.);
    w.min(h).clamp(MIN_ZOOM, 1.)
}

impl Canvas {
    pub fn is_fitted(&self) -> bool {
        self.zoom.is_none()
    }

    pub fn zoom(&self, area: (f32, f32), content: (f32, f32), margin: f32) -> f32 {
        self.zoom.unwrap_or_else(|| fit_zoom(area, content, margin))
    }

    pub fn placement(&self, area: (f32, f32), content: (f32, f32), margin: f32) -> Placement {
        let zoom = self.zoom(area, content, margin);
        let (width, height) = (content.0 * zoom, content.1 * zoom);
        Placement {
            x: area.0 / 2. + self.offset.0 - width / 2.,
            y: area.1 / 2. + self.offset.1 - height / 2.,
            width,
            height,
        }
    }

    pub fn fit(&mut self) {
        *self = Canvas::default();
    }

    pub fn zoom_to(
        &mut self,
        zoom: f32,
        at: (f32, f32),
        area: (f32, f32),
        content: (f32, f32),
        margin: f32,
    ) {
        let old = self.zoom(area, content, margin);
        let new = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let ratio = new / old;
        let center = (area.0 / 2. + self.offset.0, area.1 / 2. + self.offset.1);
        let next = (
            at.0 - (at.0 - center.0) * ratio,
            at.1 - (at.1 - center.1) * ratio,
        );
        self.zoom = Some(new);
        self.offset = (next.0 - area.0 / 2., next.1 - area.1 / 2.);
        self.clamp(area, content, margin);
    }

    pub fn zoom_by(
        &mut self,
        factor: f32,
        at: (f32, f32),
        area: (f32, f32),
        content: (f32, f32),
        margin: f32,
    ) {
        let zoom = self.zoom(area, content, margin) * factor;
        self.zoom_to(zoom, at, area, content, margin);
    }

    pub fn pan(&mut self, by: (f32, f32), area: (f32, f32), content: (f32, f32), margin: f32) {
        self.offset = (self.offset.0 + by.0, self.offset.1 + by.1);
        self.clamp(area, content, margin);
    }

    fn clamp(&mut self, area: (f32, f32), content: (f32, f32), margin: f32) {
        let zoom = self.zoom(area, content, margin);
        let room = |picture: f32, area: f32| ((picture - area) / 2.).max(0.);
        let (x, y) = (
            room(content.0 * zoom, area.0),
            room(content.1 * zoom, area.1),
        );
        self.offset = (self.offset.0.clamp(-x, x), self.offset.1.clamp(-y, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: (f32, f32) = (1000., 800.);
    const MARGIN: f32 = 24.;

    #[test]
    fn a_large_picture_is_fitted_to_the_area_less_its_margins() {
        let canvas = Canvas::default();
        let place = canvas.placement(AREA, (4000., 2000.), MARGIN);
        assert_eq!(canvas.zoom(AREA, (4000., 2000.), MARGIN), 952. / 4000.);
        assert_eq!(place.width, 952.);
        assert_eq!(place.x, 24.);
        assert_eq!(place.y + place.height / 2., 400.);
    }

    #[test]
    fn a_small_picture_keeps_its_pixels_when_fitted() {
        assert_eq!(fit_zoom(AREA, (32., 32.), MARGIN), 1.);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let content = (4000., 4000.);
        let mut canvas = Canvas::default();
        canvas.zoom_to(1., (500., 400.), AREA, content, MARGIN);
        let before = canvas.placement(AREA, content, MARGIN);
        let at = (700., 300.);
        let picture = ((at.0 - before.x) / 1., (at.1 - before.y) / 1.);
        canvas.zoom_by(2., at, AREA, content, MARGIN);
        let after = canvas.placement(AREA, content, MARGIN);
        assert_eq!(after.x + picture.0 * 2., at.0);
        assert_eq!(after.y + picture.1 * 2., at.1);
    }

    #[test]
    fn the_zoom_stays_within_its_bounds() {
        let content = (100., 100.);
        let mut canvas = Canvas::default();
        canvas.zoom_to(1000., (0., 0.), AREA, content, MARGIN);
        assert_eq!(canvas.zoom(AREA, content, MARGIN), MAX_ZOOM);
        canvas.zoom_to(0.0001, (0., 0.), AREA, content, MARGIN);
        assert_eq!(canvas.zoom(AREA, content, MARGIN), MIN_ZOOM);
    }

    #[test]
    fn a_pan_stops_at_the_edges_and_a_small_picture_stays_centred() {
        let content = (2000., 400.);
        let mut canvas = Canvas::default();
        canvas.zoom_to(1., (500., 400.), AREA, content, MARGIN);
        canvas.pan((5000., 5000.), AREA, content, MARGIN);
        let place = canvas.placement(AREA, content, MARGIN);
        assert_eq!(place.x, 0.);
        assert_eq!(place.y, 200.);
        canvas.pan((-9000., 0.), AREA, content, MARGIN);
        assert_eq!(canvas.placement(AREA, content, MARGIN).x, -1000.);
    }

    #[test]
    fn fit_returns_to_the_fitted_zoom() {
        let mut canvas = Canvas::default();
        canvas.zoom_to(3., (10., 10.), AREA, (500., 500.), MARGIN);
        assert!(!canvas.is_fitted());
        canvas.fit();
        assert!(canvas.is_fitted());
        assert_eq!(canvas, Canvas::default());
    }
}
