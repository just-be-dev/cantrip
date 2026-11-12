//! The faces the initials are cut in: each one's capitals only, as initialis keeps them
//! (`render/fonts/caps/<face>.json`: `{ upm, A: [advance, path], … }`, the outlines exactly as
//! in the font, y down). All are under the SIL Open Font License (`caps/OFL.txt`,
//! `caps/README.md`).

use std::collections::HashMap;
use std::sync::OnceLock;

use super::geom::P;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    /// Blackletter: UnifrakturMaguntia.
    Fraktur,
    /// Roman capitals with flourishes: Cinzel Decorative Bold.
    Roman,
}

pub const FACES: [Face; 2] = [Face::Fraktur, Face::Roman];

impl Face {
    fn source(self) -> &'static str {
        match self {
            Face::Fraktur => include_str!("caps/unifrakturmaguntia-400.json"),
            Face::Roman => include_str!("caps/cinzel-decorative-700.json"),
        }
    }
}

/// A face's capitals, parsed once: each letter's contours, in font units.
pub struct Caps {
    letters: HashMap<char, Vec<Vec<P>>>,
    /// The height of its capitals (an H, E or I), for sizing a letter by.
    pub cap_height: f32,
}

pub fn caps(face: Face) -> &'static Caps {
    static ALL: OnceLock<HashMap<Face, Caps>> = OnceLock::new();
    &ALL.get_or_init(|| FACES.iter().map(|&f| (f, parse(f.source()))).collect())[&face]
}

impl Caps {
    /// The outline of `letter` (A to Z), or none.
    pub fn letter(&self, letter: char) -> Option<&Vec<Vec<P>>> {
        self.letters.get(&letter.to_ascii_uppercase())
    }
}

fn parse(json: &str) -> Caps {
    let v: serde_json::Value = serde_json::from_str(json).expect("a caps face");
    let upm = v["upm"].as_f64().unwrap_or(1000.0) as f32;
    let k = 1000.0 / upm;
    let mut letters = HashMap::new();
    for (key, val) in v.as_object().into_iter().flatten() {
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            continue;
        };
        if let Some(path) = val.get(1).and_then(|p| p.as_str()) {
            letters.insert(c, flatten(path, k));
        }
    }
    let cap_height = ['H', 'E', 'I']
        .iter()
        .find_map(|c| letters.get(c))
        .map(|cs: &Vec<Vec<P>>| {
            let ys = cs.iter().flatten().map(|p| p[1]);
            let (lo, hi) = ys.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
            hi - lo
        })
        .unwrap_or(700.0);
    Caps {
        letters,
        cap_height,
    }
}

/// SVG path data (absolute M, L, Q, C and Z, each with its own arguments) as closed polylines.
fn flatten(d: &str, k: f32) -> Vec<Vec<P>> {
    let mut tokens = Vec::new();
    let mut num = String::new();
    let flush = |num: &mut String, tokens: &mut Vec<Token>| {
        if !num.is_empty() {
            tokens.push(Token::Num(num.parse().unwrap_or(0.0)));
            num.clear();
        }
    };
    for ch in d.chars() {
        match ch {
            'M' | 'L' | 'Q' | 'C' | 'Z' => {
                flush(&mut num, &mut tokens);
                tokens.push(Token::Cmd(ch));
            }
            '-' => {
                // A minus starts a number, except in an exponent.
                if !num.ends_with('e') {
                    flush(&mut num, &mut tokens);
                }
                num.push(ch);
            }
            '0'..='9' | 'e' => num.push(ch),
            '.' => {
                // A second point starts another number.
                if num.contains('.') && !num.contains('e') {
                    flush(&mut num, &mut tokens);
                }
                num.push(ch);
            }
            _ => flush(&mut num, &mut tokens),
        }
    }
    flush(&mut num, &mut tokens);

    let mut out = Vec::new();
    let mut cur: Vec<P> = Vec::new();
    let mut at = [0.0f32; 2];
    let mut i = 0;
    let args = |i: &mut usize, n: usize| -> Vec<f32> {
        let v = tokens[*i..(*i + n).min(tokens.len())]
            .iter()
            .map(|t| match t {
                Token::Num(x) => *x * k,
                Token::Cmd(_) => 0.0,
            })
            .collect();
        *i += n;
        v
    };
    while i < tokens.len() {
        let Token::Cmd(c) = tokens[i] else {
            i += 1;
            continue;
        };
        i += 1;
        match c {
            'M' => {
                if cur.len() > 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                let v = args(&mut i, 2);
                at = [v[0], v[1]];
                cur.push(at);
            }
            'L' => {
                let v = args(&mut i, 2);
                at = [v[0], v[1]];
                cur.push(at);
            }
            'Q' => {
                let v = args(&mut i, 4);
                let (c1, to) = ([v[0], v[1]], [v[2], v[3]]);
                let l = (c1[0] - at[0]).hypot(c1[1] - at[1]) + (to[0] - c1[0]).hypot(to[1] - c1[1]);
                let n = ((l / 7.0).ceil() as usize).clamp(2, 24);
                let p0 = at;
                for s in 1..=n {
                    let t = s as f32 / n as f32;
                    let u = 1.0 - t;
                    cur.push([
                        u * u * p0[0] + 2.0 * u * t * c1[0] + t * t * to[0],
                        u * u * p0[1] + 2.0 * u * t * c1[1] + t * t * to[1],
                    ]);
                }
                at = to;
            }
            'C' => {
                let v = args(&mut i, 6);
                let (c1, c2, to) = ([v[0], v[1]], [v[2], v[3]], [v[4], v[5]]);
                let l = (c1[0] - at[0]).hypot(c1[1] - at[1])
                    + (c2[0] - c1[0]).hypot(c2[1] - c1[1])
                    + (to[0] - c2[0]).hypot(to[1] - c2[1]);
                let n = ((l / 7.0).ceil() as usize).clamp(2, 32);
                let p0 = at;
                for s in 1..=n {
                    let t = s as f32 / n as f32;
                    let u = 1.0 - t;
                    let b = |a: f32, b: f32, c: f32, d: f32| {
                        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
                    };
                    cur.push([b(p0[0], c1[0], c2[0], to[0]), b(p0[1], c1[1], c2[1], to[1])]);
                }
                at = to;
            }
            _ => {
                if cur.len() > 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() > 2 {
        out.push(cur);
    }
    out
}

#[derive(Clone, Copy)]
enum Token {
    Cmd(char),
    Num(f32),
}
