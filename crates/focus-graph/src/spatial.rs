pub(crate) struct Grid {
    origin: [f32; 2],
    cell: f32,
    cols: usize,
    rows: usize,
    starts: Vec<u32>,
    items: Vec<u32>,
    pos: Vec<[f32; 2]>,
}

impl Grid {
    pub fn new(positions: &[[f32; 2]]) -> Grid {
        let b = crate::layout::bounds(positions);
        let (w, h) = ((b[2] - b[0]).max(1.0), (b[3] - b[1]).max(1.0));
        let cell = ((w * h) / (positions.len().max(1) as f32 / 2.0))
            .sqrt()
            .max(w.max(h) / 1024.0)
            .max(1e-3);
        let cols = ((w / cell) as usize + 1).min(1025);
        let rows = ((h / cell) as usize + 1).min(1025);
        let mut grid = Grid {
            origin: [b[0], b[1]],
            cell,
            cols,
            rows,
            starts: vec![0; cols * rows + 1],
            items: vec![0; positions.len()],
            pos: positions.to_vec(),
        };
        let keys: Vec<usize> = positions.iter().map(|p| grid.key(*p)).collect();
        for &k in &keys {
            grid.starts[k + 1] += 1;
        }
        for i in 1..grid.starts.len() {
            grid.starts[i] += grid.starts[i - 1];
        }
        let mut at = grid.starts.clone();
        for (i, &k) in keys.iter().enumerate() {
            grid.items[at[k] as usize] = i as u32;
            at[k] += 1;
        }
        grid
    }

    fn col_row(&self, p: [f32; 2]) -> (usize, usize) {
        let c = ((p[0] - self.origin[0]) / self.cell)
            .floor()
            .clamp(0.0, (self.cols - 1) as f32) as usize;
        let r = ((p[1] - self.origin[1]) / self.cell)
            .floor()
            .clamp(0.0, (self.rows - 1) as f32) as usize;
        (c, r)
    }

    fn key(&self, p: [f32; 2]) -> usize {
        let (c, r) = self.col_row(p);
        r * self.cols + c
    }

    fn cells(&self, rect: [f32; 4]) -> impl Iterator<Item = usize> + '_ {
        let (c0, r0) = self.col_row([rect[0], rect[1]]);
        let (c1, r1) = self.col_row([rect[2], rect[3]]);
        (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| r * self.cols + c))
    }

    fn cell_items(&self, k: usize) -> &[u32] {
        &self.items[self.starts[k] as usize..self.starts[k + 1] as usize]
    }

    pub fn nearest(&self, point: [f32; 2], radius: f32) -> Option<u32> {
        let rect = [
            point[0] - radius,
            point[1] - radius,
            point[0] + radius,
            point[1] + radius,
        ];
        let mut best: Option<(f32, u32)> = None;
        for k in self.cells(rect) {
            for &i in self.cell_items(k) {
                let p = self.pos[i as usize];
                let d2 = (p[0] - point[0]).powi(2) + (p[1] - point[1]).powi(2);
                if d2 <= radius * radius && best.is_none_or(|(b, _)| d2 < b) {
                    best = Some((d2, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    pub fn in_rect(&self, rect: [f32; 4], out: &mut Vec<u32>) {
        out.clear();
        if rect[0] > rect[2] || rect[1] > rect[3] {
            return;
        }
        for k in self.cells(rect) {
            out.extend(self.cell_items(k).iter().copied().filter(|&i| {
                let p = self.pos[i as usize];
                p[0] >= rect[0] && p[0] <= rect[2] && p[1] >= rect[1] && p[1] <= rect[3]
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_and_rect_match_brute_force() {
        let pos: Vec<[f32; 2]> = (0..2000)
            .map(|i| {
                [
                    ((i * 37) % 1000) as f32 * 1.3,
                    ((i * 91) % 700) as f32 * 0.7,
                ]
            })
            .collect();
        let g = Grid::new(&pos);
        for (q, r) in [
            ([100.0, 100.0], 15.0),
            ([640.0, 33.0], 40.0),
            ([-500.0, -500.0], 10.0),
        ] {
            let brute = pos
                .iter()
                .enumerate()
                .map(|(i, p)| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2), i as u32))
                .filter(|(d, _)| *d <= r * r)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, i)| i);
            let got = g.nearest(q, r);
            assert_eq!(got.map(|i| pos[i as usize]), brute.map(|i| pos[i as usize]));
        }
        let rect = [200.0, 100.0, 450.0, 300.0];
        let mut out = Vec::new();
        g.in_rect(rect, &mut out);
        out.sort_unstable();
        let brute: Vec<u32> = pos
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p[0] >= rect[0] && p[0] <= rect[2] && p[1] >= rect[1] && p[1] <= rect[3]
            })
            .map(|(i, _)| i as u32)
            .collect();
        assert_eq!(out, brute);
    }
}
