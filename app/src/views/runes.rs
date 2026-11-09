//! Sigil's runes: sixteen glyphs of straight strokes in a 6 × 14 box, y down. Every letter has
//! one (the same letter always the same rune, a cipher of the query), and the wand wears a ring
//! of them in place of orbiting stars.

use rune::{Color, El, Shape, canvas};

type Stroke = &'static [[f32; 2]];

pub const GLYPHS: [&[Stroke]; 16] = [
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[0.0, 2.0], [6.0, 6.0]],
        &[[0.0, 8.0], [6.0, 4.0]],
    ],
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[0.0, 0.0], [6.0, 5.0], [0.0, 9.0]],
    ],
    &[
        &[[3.0, 0.0], [3.0, 14.0]],
        &[[3.0, 4.0], [0.0, 0.0]],
        &[[3.0, 4.0], [6.0, 0.0]],
    ],
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[6.0, 0.0], [6.0, 14.0]],
        &[[0.0, 6.0], [6.0, 10.0]],
    ],
    &[
        &[[0.0, 14.0], [3.0, 0.0], [6.0, 14.0]],
        &[[1.0, 8.0], [5.0, 8.0]],
    ],
    &[&[[0.0, 0.0], [6.0, 14.0]], &[[6.0, 0.0], [0.0, 14.0]]],
    &[&[[3.0, 0.0], [3.0, 14.0]], &[[0.0, 4.0], [6.0, 8.0]]],
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[0.0, 7.0], [5.0, 2.0]],
        &[[0.0, 7.0], [5.0, 12.0]],
    ],
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[0.0, 0.0], [6.0, 4.0], [0.0, 8.0], [6.0, 12.0]],
    ],
    &[&[[3.0, 0.0], [6.0, 7.0], [3.0, 14.0], [0.0, 7.0], [3.0, 0.0]]],
    &[
        &[[0.0, 0.0], [0.0, 14.0]],
        &[[6.0, 0.0], [6.0, 14.0]],
        &[[0.0, 0.0], [6.0, 14.0]],
    ],
    &[
        &[[3.0, 0.0], [3.0, 14.0]],
        &[[0.0, 3.0], [3.0, 6.0], [6.0, 3.0]],
    ],
    &[
        &[[0.0, 14.0], [0.0, 0.0], [6.0, 7.0], [6.0, 0.0]],
        &[[6.0, 7.0], [6.0, 14.0]],
    ],
    &[
        &[[0.0, 4.0], [3.0, 0.0], [6.0, 4.0]],
        &[[3.0, 0.0], [3.0, 14.0]],
        &[[0.0, 10.0], [6.0, 10.0]],
    ],
    &[
        &[[0.0, 0.0], [3.0, 5.0], [6.0, 0.0]],
        &[[3.0, 5.0], [3.0, 14.0]],
        &[[0.0, 14.0], [6.0, 9.0]],
    ],
    &[
        &[[0.0, 0.0], [6.0, 0.0]],
        &[[3.0, 0.0], [3.0, 14.0]],
        &[[0.0, 7.0], [6.0, 11.0]],
    ],
];

/// The rune for `c`.
pub fn of(c: char) -> usize {
    let lower = c.to_lowercase().next().unwrap_or(c) as u32;
    (lower.wrapping_mul(7) % GLYPHS.len() as u32) as usize
}

/// Glyph `index`'s strokes, `height` points tall, centred on `at` and turned by `angle` radians.
pub fn strokes(index: usize, at: [f32; 2], height: f32, angle: f32) -> Vec<Vec<[f32; 2]>> {
    let k = height / 14.0;
    let (s, c) = angle.sin_cos();
    GLYPHS[index % GLYPHS.len()]
        .iter()
        .map(|stroke| {
            stroke
                .iter()
                .map(|&[x, y]| {
                    let (x, y) = ((x - 3.0) * k, (y - 7.0) * k);
                    [at[0] + x * c - y * s, at[1] + x * s + y * c]
                })
                .collect()
        })
        .collect()
}

/// Rune `index` as an element `height` points tall, in `color`, with a soft glow under its
/// strokes. Its box is a little larger than the glyph, centred on it.
pub fn view(index: usize, height: f32, color: Color) -> El {
    let (w, h) = (height * 0.8 + 6.0, height + 6.0);
    let mut shape = Shape::new();
    let lines = strokes(index, [w / 2.0, h / 2.0], height, 0.0);
    for line in &lines {
        shape = shape.stroke(line.clone(), 3.2, color.alpha(color.a * 0.22));
    }
    for line in lines {
        shape = shape.stroke(line, 1.25, color);
    }
    canvas(shape).size(w, h)
}

/// The size of `view(_, height, _)`'s box.
pub fn view_size(height: f32) -> (f32, f32) {
    (height * 0.8 + 6.0, height + 6.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn letters_keep_their_runes_and_cases_share_them() {
        assert_eq!(super::of('s'), super::of('S'));
        assert_eq!(super::of('s'), super::of('s'));
        let distinct: std::collections::HashSet<_> = "spell".chars().map(super::of).collect();
        assert!(distinct.len() >= 3, "a word reads as several runes");
    }
}
