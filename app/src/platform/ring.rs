//! The `ring` menu bar icon: a circle that fills clockwise from 12 o'clock, drawn in Rust
//! (supersampled coverage) so frames are cheap and testable. Ported from the Swift RingIcon.

use crate::protocol::WireTone;

/// The icon's side in points.
pub const SIZE: f64 = 18.0;
const RADIUS: f64 = 6.8;
const LINE: f64 = 2.4;
const SAMPLES: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RingFrame {
    /// 0–1 to draw, or `None` when there's no data yet (dashed track).
    pub value: Option<f64>,
    /// Rotation of the busy segment in radians, or `None` when not busy.
    pub spinner: Option<f64>,
    pub badge: bool,
    pub tone: WireTone,
}

impl RingFrame {
    /// The colour the ring draws in. Normal is black, drawn as a template image the menu bar
    /// tints for light and dark.
    pub fn base(&self) -> [f64; 3] {
        match self.tone {
            WireTone::Normal => [0.0, 0.0, 0.0],
            WireTone::Warning => [1.0, 0.584, 0.0],
            WireTone::Critical => [1.0, 0.231, 0.188],
        }
    }
}

/// Straight-alpha RGBA at `scale` pixels per point, `SIZE` points square.
pub fn rgba(frame: &RingFrame, scale: f64) -> (u32, u32, Vec<u8>) {
    let px = (SIZE * scale).round() as usize;
    let base = frame.base();
    let mut out = Vec::with_capacity(px * px * 4);
    for y in 0..px {
        for x in 0..px {
            let mut alpha = 0.0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let p = (
                        (x as f64 + (sx as f64 + 0.5) / SAMPLES as f64) / scale,
                        (y as f64 + (sy as f64 + 0.5) / SAMPLES as f64) / scale,
                    );
                    alpha += sample(frame, p);
                }
            }
            alpha /= (SAMPLES * SAMPLES) as f64;
            let a = (alpha * 255.0).round() as u8;
            out.extend_from_slice(&[
                (base[0] * 255.0) as u8,
                (base[1] * 255.0) as u8,
                (base[2] * 255.0) as u8,
                a,
            ]);
        }
    }
    (px as u32, px as u32, out)
}

/// The opacity at point `p` (points, y down): every layer composited "over" the ones below.
fn sample(f: &RingFrame, p: (f64, f64)) -> f64 {
    let c = SIZE / 2.0;
    let (dx, dy) = (p.0 - c, p.1 - c);
    let d = (dx * dx + dy * dy).sqrt();
    // Clockwise from 12 o'clock, in radians, 0..2π (y grows down).
    let theta = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU);
    let mut a = 0.0;
    let mut over = |layer: f64| a = layer + a * (1.0 - layer);

    let in_band = (d - RADIUS).abs() <= LINE / 2.0;
    if in_band {
        let on = match f.value {
            Some(_) => true,
            None => (theta * RADIUS).rem_euclid(3.8) < 1.6,
        };
        if on {
            over(0.28);
        }
    }
    if let Some(v) = f.value.filter(|v| *v > 0.005)
        && on_arc(p, d, theta, 0.0, std::f64::consts::TAU * v.min(1.0))
    {
        over(1.0);
    }
    if let Some(angle) = f.spinner
        && on_arc(
            p,
            d,
            theta,
            angle.rem_euclid(std::f64::consts::TAU),
            70f64.to_radians(),
        )
    {
        over(if f.value.is_none() { 1.0 } else { 0.55 });
    }
    if f.badge && d <= 1.8 {
        over(1.0);
    }
    a
}

/// On the arc from `start` sweeping `sweep` radians clockwise, with round caps.
fn on_arc(p: (f64, f64), d: f64, theta: f64, start: f64, sweep: f64) -> bool {
    let tau = std::f64::consts::TAU;
    if (d - RADIUS).abs() <= LINE / 2.0 && (theta - start).rem_euclid(tau) <= sweep {
        return true;
    }
    let c = SIZE / 2.0;
    [start, start + sweep].iter().any(|&t| {
        let end = (c + RADIUS * t.sin(), c - RADIUS * t.cos());
        let (ex, ey) = (p.0 - end.0, p.1 - end.1);
        ex * ex + ey * ey <= (LINE / 2.0).powi(2)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(frame: &RingFrame, x: f64, y: f64) -> u8 {
        let (w, _, px) = rgba(frame, 2.0);
        let (x, y) = ((x * 2.0) as usize, (y * 2.0) as usize);
        px[(y * w as usize + x) * 4 + 3]
    }

    #[test]
    fn a_quarter_fills_from_twelve_to_three() {
        let f = RingFrame {
            value: Some(0.25),
            ..Default::default()
        };
        let c = SIZE / 2.0;
        // 1:30 is inside the quarter, 7:30 is only the faint track.
        let (s, k) = (RADIUS * std::f64::consts::FRAC_1_SQRT_2, c);
        assert_eq!(alpha_at(&f, k + s, k - s), 255);
        let faint = alpha_at(&f, k - s, k + s);
        assert!(faint > 40 && faint < 100, "{faint}");
        assert_eq!(alpha_at(&f, c, c), 0, "the centre stays clear");
    }

    #[test]
    fn no_data_draws_a_dashed_track_and_a_badge() {
        let f = RingFrame {
            value: None,
            badge: true,
            ..Default::default()
        };
        let (_, _, px) = rgba(&f, 2.0);
        let max = px.chunks(4).map(|p| p[3]).max().unwrap();
        assert_eq!(max, 255, "the badge is solid");
        assert_eq!(alpha_at(&f, SIZE / 2.0, SIZE / 2.0), 255);
    }
}
