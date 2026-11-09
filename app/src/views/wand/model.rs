//! The wand's mesh, turned from an outline, and how its materials shade.

use std::f32::consts::TAU;
use std::sync::LazyLock;

use rune::Color;

use super::super::palette::{Palette, srgb};
use super::CAMERA;
use super::motion::smoothstep;
use super::raster::{V3, add, dot, mix, mul, norm, sub};

#[derive(Clone, Copy, PartialEq)]
pub enum Material {
    Gold = 0,
    Grip = 1,
    Shaft = 2,
    Tip = 3,
}

/// The wand's outline, bottom to top, as (height, radius) runs. A run is one material with
/// smooth normals; neighbouring runs share their end point and crease there. Turned about
/// the vertical, it's the whole model: a gold pommel and collar, a wrapped grip, a gold band,
/// a tapering lacquered shaft, a gold ferrule and a glowing tip.
const PROFILE: &[(Material, &[(f32, f32)])] = &[
    (
        Material::Gold,
        &[
            (-1.05, 0.0),
            (-1.04, 0.035),
            (-1.015, 0.068),
            (-0.98, 0.086),
            (-0.945, 0.088),
            (-0.915, 0.076),
            (-0.897, 0.062),
            (-0.887, 0.083),
            (-0.866, 0.09),
            (-0.846, 0.083),
            (-0.835, 0.07),
        ],
    ),
    (
        Material::Grip,
        &[
            (-0.835, 0.07),
            (-0.74, 0.078),
            (-0.6, 0.082),
            (-0.47, 0.076),
            (-0.39, 0.068),
        ],
    ),
    (
        Material::Gold,
        &[
            (-0.39, 0.068),
            (-0.385, 0.082),
            (-0.37, 0.09),
            (-0.35, 0.09),
            (-0.335, 0.082),
            (-0.33, 0.06),
        ],
    ),
    (
        Material::Shaft,
        &[
            (-0.33, 0.06),
            (-0.1, 0.054),
            (0.2, 0.047),
            (0.5, 0.041),
            (0.84, 0.035),
        ],
    ),
    (
        Material::Gold,
        &[
            (0.84, 0.035),
            (0.845, 0.046),
            (0.865, 0.05),
            (0.885, 0.046),
            (0.89, 0.036),
        ],
    ),
    (
        Material::Tip,
        &[
            (0.89, 0.036),
            (0.94, 0.034),
            (0.98, 0.026),
            (1.005, 0.014),
            (1.015, 0.0),
        ],
    ),
];
const SEGMENTS: usize = 24;
/// Widens the outline: thin enough to read as a wand, thick enough to read at 30 points.
const GIRTH: f32 = 1.75;

pub struct Vertex {
    pub pos: V3,
    pub normal: V3,
    /// Its angle about the axis, for the grip's wrap.
    pub cos: f32,
    pub sin: f32,
    pub material: Material,
}

pub struct Mesh {
    pub verts: Vec<Vertex>,
    pub tris: Vec<[u32; 3]>,
}

pub static MESH: LazyLock<Mesh> = LazyLock::new(|| {
    let mut mesh = Mesh {
        verts: Vec::new(),
        tris: Vec::new(),
    };
    for &(material, run) in PROFILE {
        let base = mesh.verts.len() as u32;
        for (i, &(y, r)) in run.iter().enumerate() {
            let r = r * GIRTH;
            // Outward in the (radius, height) plane, from the run's local tangent.
            let (a, b) = (run[i.saturating_sub(1)], run[(i + 1).min(run.len() - 1)]);
            let (nr, ny) = if r == 0.0 {
                (0.0, if i == 0 { -1.0 } else { 1.0 })
            } else {
                let (dr, dy) = ((b.1 - a.1) * GIRTH, b.0 - a.0);
                let len = (dr * dr + dy * dy).sqrt();
                (dy / len, -dr / len)
            };
            for j in 0..SEGMENTS {
                let (sin, cos) = (j as f32 / SEGMENTS as f32 * TAU).sin_cos();
                mesh.verts.push(Vertex {
                    pos: [r * cos, y, r * sin],
                    normal: [nr * cos, ny, nr * sin],
                    cos,
                    sin,
                    material,
                });
            }
        }
        let at = |i: usize, j: usize| base + (i * SEGMENTS + j % SEGMENTS) as u32;
        for i in 0..run.len() - 1 {
            for j in 0..SEGMENTS {
                let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j), at(i + 1, j + 1));
                mesh.tris.push([a, b, d]);
                mesh.tris.push([a, d, c]);
            }
        }
    }
    mesh
});

/// Lights and materials, in the Arcane palette's linear colours.
pub struct Looks {
    gold: V3,
    grip: V3,
    wrap: V3,
    shaft: V3,
    violet: V3,
    magenta: V3,
}

impl Looks {
    pub fn new(p: &Palette) -> Looks {
        Looks {
            gold: rgb(p.amber),
            grip: rgb(srgb(0.36, 0.22, 0.56, 1.0)),
            wrap: rgb(srgb(0.13, 0.08, 0.24, 1.0)),
            shaft: rgb(srgb(0.17, 0.12, 0.30, 1.0)),
            violet: rgb(p.violet),
            magenta: rgb(p.magenta),
        }
    }

    /// Attributes: world normal, world position, height on the model, angle (cos, sin),
    /// material. Blinn-Phong under a warm key and a violet fill, with a magenta-violet rim.
    pub fn shade(&self, a: &[f32; 10]) -> [f32; 4] {
        let n = norm([a[0], a[1], a[2]]);
        let v = norm(sub(CAMERA, [a[3], a[4], a[5]]));
        let y = a[6];
        let key = norm([-0.45, 0.75, 0.6]);
        let fill = norm([0.7, -0.35, 0.45]);
        let diffuse = dot(n, key).max(0.0);
        let fill = dot(n, fill).max(0.0);
        let shine = |power: i32| dot(n, norm(add(key, v))).max(0.0).powi(power);
        let rim = (1.0 - dot(n, v).max(0.0)).powi(3);
        let white = [1.0; 3];
        let c = match a[9].round() as u8 {
            0 => {
                // Polished metal: a fake sky in the reflection does most of the work.
                let up = sub(mul(n, 2.0 * dot(n, v)), v)[1];
                let lit = 0.12 + 0.6 * diffuse + 0.5 * smoothstep(-0.2, 0.8, up);
                add(
                    add(
                        mul(self.gold, lit),
                        mul(mix(self.gold, white, 0.6), 1.2 * shine(40)),
                    ),
                    mul(self.magenta, 0.3 * rim),
                )
            }
            1 => {
                // A ribbon wound twice around the grip.
                let turn = a[8].atan2(a[7]) / TAU * 2.0 + y * 7.0;
                let s = turn - turn.floor();
                let ribbon = smoothstep(0.0, 0.06, s) * (1.0 - smoothstep(0.42, 0.48, s));
                let base = mix(self.grip, self.wrap, ribbon);
                let lit = add(
                    mul(base, 0.18 + 0.85 * diffuse),
                    mul(self.violet, 0.12 * fill),
                );
                add(
                    add(lit, mul(white, 0.18 * shine(18))),
                    mul(self.violet, 0.7 * rim),
                )
            }
            2 => {
                // Lacquer that glows warmer toward the tip, as if charged.
                let lit = add(
                    mul(self.shaft, 0.2 + 0.9 * diffuse),
                    mul(self.violet, 0.15 * fill),
                );
                let edge = mul(
                    mix(self.violet, self.magenta, smoothstep(-0.3, 0.8, y)),
                    0.9 * rim,
                );
                let charge = mul(self.magenta, 0.22 * smoothstep(0.45, 0.9, y));
                add(add(add(lit, mul(white, 0.65 * shine(60))), edge), charge)
            }
            _ => {
                let glow = mix(
                    white,
                    mul(mix(self.violet, self.magenta, 0.5), 1.1),
                    0.25 + 0.6 * rim,
                );
                mul(glow, 1.15)
            }
        };
        [c[0], c[1], c[2], 1.0]
    }
}

pub fn rgb(c: Color) -> V3 {
    [c.r, c.g, c.b]
}
