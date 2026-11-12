//! The clearance grid the ornament grows through (initialis's `render/src/field.js`): how far
//! each cell is from the nearest obstacle, capped. Every piece placed is stamped back in, and
//! a stem that comes out too short is rolled back.

use super::geom::edt;

pub struct Field {
    pub cell: f32,
    pub cap: f32,
    nx: usize,
    ny: usize,
    d: Vec<f32>,
    base: Vec<f32>,
    log: Option<Vec<(usize, f32)>>,
    // Per-block maxima, so the biggest gap is cheap to find.
    bx: usize,
    b_max: Vec<f32>,
    b_arg: Vec<usize>,
    b_dirty: Vec<bool>,
}

const B: usize = 16;

/// A stamp's changes, so a stem can be lifted out while its own leaves are placed.
pub type Log = Vec<(usize, f32)>;

impl Field {
    pub fn new(w: f32, h: f32, cell: f32, cap: f32) -> Field {
        let nx = (w / cell).ceil() as usize + 1;
        let ny = (h / cell).ceil() as usize + 1;
        let bx = nx.div_ceil(B);
        let by = ny.div_ceil(B);
        Field {
            cell,
            cap,
            nx,
            ny,
            d: vec![0.0; nx * ny],
            base: vec![0.0; nx * ny],
            log: None,
            bx,
            b_max: vec![0.0; bx * by],
            b_arg: vec![0; bx * by],
            b_dirty: vec![true; bx * by],
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        (self.nx, self.ny)
    }

    /// `blocked`: one per cell, true where nothing may go.
    pub fn set_blocked(&mut self, blocked: &[bool]) {
        let mut f: Vec<f64> = blocked
            .iter()
            .map(|&b| if b { 0.0 } else { 1e20 })
            .collect();
        edt(&mut f, self.nx, self.ny);
        for (d, f) in self.d.iter_mut().zip(&f) {
            *d = ((f.sqrt() as f32) * self.cell).min(self.cap);
        }
        self.base = self.d.clone();
        self.b_dirty.fill(true);
    }

    fn sample(&self, arr: &[f32], x: f32, y: f32) -> f32 {
        let (gx, gy) = (x / self.cell, y / self.cell);
        if gx < 0.0 || gy < 0.0 || gx >= (self.nx - 1) as f32 || gy >= (self.ny - 1) as f32 {
            return 0.0;
        }
        let (i, j) = (gx as usize, gy as usize);
        let (fx, fy) = (gx - i as f32, gy - j as f32);
        let k = j * self.nx + i;
        let (a, b, c, d) = (arr[k], arr[k + 1], arr[k + self.nx], arr[k + self.nx + 1]);
        (a + (b - a) * fx) * (1.0 - fy) + (c + (d - c) * fx) * fy
    }

    pub fn at(&self, x: f32, y: f32) -> f32 {
        self.sample(&self.d, x, y)
    }

    pub fn at_base(&self, x: f32, y: f32) -> f32 {
        self.sample(&self.base, x, y)
    }

    /// Which way is away from what's there.
    pub fn grad(&self, x: f32, y: f32) -> [f32; 2] {
        let e = self.cell;
        let gx = self.at(x + e, y) - self.at(x - e, y);
        let gy = self.at(x, y + e) - self.at(x, y - e);
        let l = gx.hypot(gy);
        let l = if l > 0.0 { l } else { 1.0 };
        [gx / l, gy / l]
    }

    pub fn begin(&mut self) {
        self.log = Some(Vec::new());
    }

    /// Stops logging, handing the log back so the change can be lifted out later.
    pub fn detach(&mut self) -> Log {
        self.log.take().unwrap_or_default()
    }

    fn dirty(&mut self, k: usize) {
        let (i, j) = (k % self.nx, k / self.nx);
        self.b_dirty[(j / B) * self.bx + i / B] = true;
    }

    pub fn lift(&mut self, log: &Log) {
        for &(k, v) in log.iter().rev() {
            self.d[k] = v;
            self.dirty(k);
        }
    }

    pub fn rollback(&mut self) {
        if let Some(log) = self.log.take() {
            self.lift(&log);
        }
    }

    /// A disc of radius `r` becomes an obstacle.
    pub fn stamp(&mut self, x: f32, y: f32, r: f32) {
        let (nx, ny, cell) = (self.nx, self.ny, self.cell);
        let reach = r + self.cap;
        let i0 = ((x - reach) / cell).floor().max(0.0) as usize;
        let i1 = (((x + reach) / cell).ceil() as isize).clamp(0, nx as isize - 1) as usize;
        let j0 = ((y - reach) / cell).floor().max(0.0) as usize;
        let j1 = (((y + reach) / cell).ceil() as isize).clamp(0, ny as isize - 1) as usize;
        if i0 > i1 || j0 > j1 {
            return;
        }
        for bj in j0 / B..=j1 / B {
            for bi in i0 / B..=i1 / B {
                self.b_dirty[bj * self.bx + bi] = true;
            }
        }
        for j in j0..=j1 {
            let dy = j as f32 * cell - y;
            for i in i0..=i1 {
                let dx = i as f32 * cell - x;
                let v = (dx * dx + dy * dy).sqrt() - r;
                let k = j * nx + i;
                if v < self.d[k] {
                    if let Some(log) = &mut self.log {
                        log.push((k, self.d[k]));
                    }
                    self.d[k] = v.max(0.0);
                }
            }
        }
    }

    /// The most open spot: where, and how far it is from anything.
    pub fn biggest(&mut self) -> (f32, f32, f32) {
        let (nx, ny) = (self.nx, self.ny);
        let by = self.b_max.len() / self.bx;
        for bj in 0..by {
            for bi in 0..self.bx {
                let b = bj * self.bx + bi;
                if !self.b_dirty[b] {
                    continue;
                }
                let (mut best, mut arg) = (-1.0, 0);
                for j in bj * B..((bj + 1) * B).min(ny) {
                    for i in bi * B..((bi + 1) * B).min(nx) {
                        let k = j * nx + i;
                        if self.d[k] > best {
                            best = self.d[k];
                            arg = k;
                        }
                    }
                }
                self.b_max[b] = best;
                self.b_arg[b] = arg;
                self.b_dirty[b] = false;
            }
        }
        let (mut best, mut k) = (-1.0, 0);
        for (b, &m) in self.b_max.iter().enumerate() {
            if m > best {
                best = m;
                k = self.b_arg[b];
            }
        }
        (
            (k % nx) as f32 * self.cell,
            (k / nx) as f32 * self.cell,
            best,
        )
    }
}
