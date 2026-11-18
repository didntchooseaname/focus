pub(crate) const MIN_ZOOM: f32 = 0.005;
pub(crate) const MAX_ZOOM: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Camera {
    pub center: [f32; 2],
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            center: [0.0, 0.0],
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn to_screen(self, p: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.center[0]) * self.zoom + size[0] / 2.0,
            (p[1] - self.center[1]) * self.zoom + size[1] / 2.0,
        ]
    }

    pub fn to_world(self, s: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        [
            (s[0] - size[0] / 2.0) / self.zoom + self.center[0],
            (s[1] - size[1] / 2.0) / self.zoom + self.center[1],
        ]
    }

    pub fn visible(self, size: [f32; 2], margin: f32) -> [f32; 4] {
        let a = self.to_world([-margin, -margin], size);
        let b = self.to_world([size[0] + margin, size[1] + margin], size);
        [a[0], a[1], b[0], b[1]]
    }

    pub fn zoom_at(&mut self, anchor: [f32; 2], factor: f32, size: [f32; 2]) {
        let before = self.to_world(anchor, size);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.to_world(anchor, size);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    pub fn pan(&mut self, screen_delta: [f32; 2]) {
        self.center[0] -= screen_delta[0] / self.zoom;
        self.center[1] -= screen_delta[1] / self.zoom;
    }

    pub fn fit(bounds: [f32; 4], size: [f32; 2], padding: f32) -> Camera {
        let w = (bounds[2] - bounds[0]).max(1.0);
        let h = (bounds[3] - bounds[1]).max(1.0);
        let zoom = ((size[0] - 2.0 * padding).max(1.0) / w)
            .min((size[1] - 2.0 * padding).max(1.0) / h)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        Camera {
            center: [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0],
            zoom,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Follow {
    velocity: [f32; 3],
}

impl Follow {
    pub fn is_still(&self) -> bool {
        self.velocity == [0.0; 3]
    }

    pub fn step(
        &mut self,
        camera: Camera,
        target: Camera,
        smooth: f32,
        dt: f32,
        max_pan: Option<f32>,
    ) -> (Camera, bool) {
        let from = [camera.center[0], camera.center[1], camera.zoom.ln()];
        let to = [target.center[0], target.center[1], target.zoom.ln()];
        let omega = 2.0 / smooth.max(f32::EPSILON);
        let x = omega * dt;
        let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
        let mut at = [0.0f32; 3];
        for k in 0..3 {
            let change = from[k] - to[k];
            let pull = (self.velocity[k] + omega * change) * dt;
            self.velocity[k] = (self.velocity[k] - omega * pull) * decay;
            at[k] = to[k] + (change + pull) * decay;
        }
        if let Some(max) = max_pan.filter(|_| dt > 0.0) {
            let zoom = at[2].exp();
            let moved = [at[0] - from[0], at[1] - from[1]];
            let pixels = moved[0].hypot(moved[1]) * zoom;
            if pixels > max * dt {
                let k = max * dt / pixels;
                for c in 0..2 {
                    at[c] = from[c] + moved[c] * k;
                    self.velocity[c] = moved[c] * k / dt;
                }
            }
        }
        let zoom = at[2].exp().clamp(MIN_ZOOM, MAX_ZOOM);
        let gap = (at[0] - to[0]).hypot(at[1] - to[1]) * zoom;
        let speed = self.velocity[0].hypot(self.velocity[1]) * zoom;
        if gap < 0.25
            && (at[2] - to[2]).abs() < 1e-3
            && speed < 1.0
            && self.velocity[2].abs() < 1e-2
        {
            *self = Follow::default();
            return (target, false);
        }
        (
            Camera {
                center: [at[0], at[1]],
                zoom,
            },
            true,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_eases_without_jump_or_overshoot_and_stops() {
        let mut follow = Follow::default();
        let mut camera = Camera {
            center: [0.0, 0.0],
            zoom: 1.0,
        };
        let target = Camera {
            center: [600.0, 0.0],
            zoom: 0.25,
        };
        let dt = 1.0 / 320.0;
        let mut steps = 0;
        let mut moving = true;
        let mut last_pan = 0.0f32;
        let mut largest_pan = 0.0f32;
        let mut largest_zoom = 0.0f32;
        while moving && steps < 3200 {
            let (next, still) = follow.step(camera, target, 0.5, dt, None);
            let pan = (next.center[0] - camera.center[0]).abs() * next.zoom;
            assert!(next.center[0] <= target.center[0] + 1e-3, "no overshoot");
            assert!(next.zoom >= target.zoom * 0.999, "no overshoot");
            assert!(
                pan <= last_pan + 0.5,
                "step {steps}: {pan} px after {last_pan} px"
            );
            largest_pan = largest_pan.max(pan);
            largest_zoom = largest_zoom.max((next.zoom / camera.zoom - 1.0).abs());
            last_pan = pan;
            camera = next;
            moving = still;
            steps += 1;
        }
        assert!(!moving, "the camera stops on its target");
        assert_eq!(camera, target);
        assert!(steps < 3 * 320, "{steps} frames at 320 Hz");
        assert!(largest_pan < 4.0, "{largest_pan} px in one frame at 320 Hz");
        assert!(
            largest_zoom < 0.02,
            "{largest_zoom} zoom change in one frame"
        );
    }

    #[test]
    fn a_capped_follow_pans_no_faster_than_asked_and_still_stops() {
        let mut follow = Follow::default();
        let mut camera = Camera::default();
        let target = Camera {
            center: [2000.0, 0.0],
            zoom: 1.0,
        };
        let dt = 1.0 / 60.0;
        let mut moving = true;
        let mut steps = 0;
        while moving && steps < 6000 {
            let (next, still) = follow.step(camera, target, 0.5, dt, Some(180.0));
            let pan = (next.center[0] - camera.center[0]).abs() * next.zoom;
            assert!(pan <= 180.0 * dt + 1e-3, "step {steps}: {pan} px");
            assert!(next.center[0] <= target.center[0] + 1e-3, "no overshoot");
            camera = next;
            moving = still;
            steps += 1;
        }
        assert_eq!(camera, target);
    }

    #[test]
    fn round_trip_and_anchored_zoom() {
        let size = [800.0, 600.0];
        let mut c = Camera {
            center: [10.0, -5.0],
            zoom: 0.5,
        };
        let p = [123.0, 45.0];
        let s = c.to_screen(p, size);
        let back = c.to_world(s, size);
        assert!((back[0] - p[0]).abs() < 1e-3 && (back[1] - p[1]).abs() < 1e-3);
        let anchor = [200.0, 100.0];
        let w = c.to_world(anchor, size);
        c.zoom_at(anchor, 2.0, size);
        let w2 = c.to_world(anchor, size);
        assert!((w[0] - w2[0]).abs() < 1e-3 && (w[1] - w2[1]).abs() < 1e-3);
        assert_eq!(c.zoom, 1.0);
        let fit = Camera::fit([0.0, 0.0, 100.0, 50.0], size, 40.0);
        let tl = fit.to_screen([0.0, 0.0], size);
        assert!(tl[0] >= 39.0 && tl[1] >= 39.0, "{tl:?}");
    }
}
