//! Minecraft's blocks drawn in Elden Ring (sheet: systems.block_render, first version).
//!
//! Minecraft ships its block meshes and atlas over the render ring. This first version draws each
//! triangle with Elden Ring's debug draw (CSEzDraw) in one flat colour: the atlas texel at the
//! triangle's centre times Minecraft's tint, shaded per face like Minecraft does. A textured D3D12
//! renderer replaces it later.

use std::collections::HashMap;

use eldenring::cs::{CSEzDraw, EzDrawFillMode};
use eldenring::position::HavokPosition;
use fromsoftware_shared::{F32Vector4, Triangle};

use crate::coords::Frame;
use crate::link::Link;
use crate::proto::*;

const DRAW_RADIUS: f64 = 48.0;
const MAX_TRIS_PER_FRAME: usize = 8000;
const DRAIN_BYTES_PER_FRAME: u64 = 16 << 20;

struct Tri {
    p: [[f32; 3]; 3], // absolute Minecraft coords
    centre: [f32; 3],
    color: [f32; 4],
}

pub struct Blocks {
    atlas_w: u32,
    atlas_h: u32,
    atlas: Vec<u8>,
    sections: HashMap<(i32, i32, i32), Vec<Tri>>,
    pub messages: u64,
}

fn read<T: Copy>(b: &[u8], off: usize) -> Option<T> {
    if off + size_of::<T>() > b.len() {
        return None;
    }
    Some(unsafe { (b.as_ptr().add(off) as *const T).read_unaligned() })
}

fn rgba(c: u32) -> [f32; 4] {
    [
        (c & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        ((c >> 16) & 0xFF) as f32 / 255.0,
        ((c >> 24) & 0xFF) as f32 / 255.0,
    ]
}

impl Blocks {
    pub fn new() -> Self {
        Blocks { atlas_w: 0, atlas_h: 0, atlas: Vec::new(), sections: HashMap::new(), messages: 0 }
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    fn texel(&self, u: f32, v: f32) -> [f32; 4] {
        if self.atlas_w == 0 {
            return [0.6, 0.6, 0.6, 1.0];
        }
        let x = ((u.clamp(0.0, 0.9999)) * self.atlas_w as f32) as usize;
        let y = ((v.clamp(0.0, 0.9999)) * self.atlas_h as f32) as usize;
        let i = (y * self.atlas_w as usize + x) * 4;
        if i + 3 >= self.atlas.len() {
            return [0.6, 0.6, 0.6, 1.0];
        }
        [
            self.atlas[i] as f32 / 255.0,
            self.atlas[i + 1] as f32 / 255.0,
            self.atlas[i + 2] as f32 / 255.0,
            self.atlas[i + 3] as f32 / 255.0,
        ]
    }

    pub fn drain(&mut self, link: &Link) {
        let mut msgs: Vec<(u32, Vec<u8>)> = Vec::new();
        link.drain_render(DRAIN_BYTES_PER_FRAME, |kind, payload| {
            if matches!(kind, REN_ATLAS | REN_SECTION | REN_CLEAR_ALL | REN_ATLAS_REGION) {
                msgs.push((kind, payload.to_vec()));
            }
        });
        for (kind, p) in msgs {
            self.messages += 1;
            match kind {
                REN_ATLAS => {
                    let (Some(w), Some(h)) = (read::<u32>(&p, 0), read::<u32>(&p, 4)) else { continue };
                    let bytes = (w as usize) * (h as usize) * 4;
                    if p.len() >= 8 + bytes {
                        self.atlas_w = w;
                        self.atlas_h = h;
                        self.atlas = p[8..8 + bytes].to_vec();
                        crate::log!("atlas {w}x{h}");
                    }
                }
                REN_ATLAS_REGION => {} // animated textures: not needed for flat colours
                REN_CLEAR_ALL => self.sections.clear(),
                REN_SECTION => {
                    let Some(s) = read::<RenSection>(&p, 0) else { continue };
                    let key = (s.sx, s.sy, s.sz);
                    if s.vertex_count == 0 {
                        self.sections.remove(&key);
                        continue;
                    }
                    let origin = [(s.sx * 16) as f32, (s.sy * 16) as f32, (s.sz * 16) as f32];
                    let mut tris = Vec::with_capacity(s.vertex_count as usize / 3);
                    let base = size_of::<RenSection>();
                    for t in 0..(s.vertex_count as usize / 3) {
                        let mut vs = [[0f32; 3]; 3];
                        let (mut u, mut v) = (0f32, 0f32);
                        let mut tint = [0f32; 4];
                        let mut flags = 0u32;
                        let mut ok = true;
                        for k in 0..3 {
                            let Some(vx) = read::<RenVertex>(&p, base + (t * 3 + k) * size_of::<RenVertex>()) else {
                                ok = false;
                                break;
                            };
                            vs[k] = [origin[0] + vx.x, origin[1] + vx.y, origin[2] + vx.z];
                            u += vx.u / 3.0;
                            v += vx.v / 3.0;
                            let c = rgba(vx.color);
                            for i in 0..4 {
                                tint[i] += c[i] / 3.0;
                            }
                            flags = vx.flags;
                        }
                        if !ok {
                            break;
                        }
                        let tex = self.texel(u, v);
                        if tex[3] < 0.1 {
                            continue; // fully transparent texel (cutout leaves, glass edges)
                        }
                        // Minecraft's fixed face shading: down, up, north, south, west, east.
                        let shade = match (flags >> 4) & 7 {
                            1 => 0.5,
                            2 => 1.0,
                            3 | 4 => 0.8,
                            5 | 6 => 0.6,
                            _ => 0.9,
                        };
                        let color = [tex[0] * tint[0] * shade, tex[1] * tint[1] * shade, tex[2] * tint[2] * shade, 1.0];
                        let centre = [
                            (vs[0][0] + vs[1][0] + vs[2][0]) / 3.0,
                            (vs[0][1] + vs[1][1] + vs[2][1]) / 3.0,
                            (vs[0][2] + vs[1][2] + vs[2][2]) / 3.0,
                        ];
                        tris.push(Tri { p: vs, centre, color });
                    }
                    self.sections.insert(key, tris);
                }
                _ => {}
            }
        }
    }

    /// Returns how many triangles were drawn.
    pub fn draw(&self, ez: &mut CSEzDraw, frame: &Frame, mc_feet: [f64; 3], selection: Option<([f32; 3], [f32; 3])>) -> usize {
        ez.set_fill_mode(EzDrawFillMode::Fill);
        let mut drawn = 0usize;
        let r2 = DRAW_RADIUS * DRAW_RADIUS;
        'outer: for tris in self.sections.values() {
            for t in tris {
                let d = [
                    t.centre[0] as f64 - mc_feet[0],
                    t.centre[1] as f64 - mc_feet[1],
                    t.centre[2] as f64 - mc_feet[2],
                ];
                if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > r2 {
                    continue;
                }
                let h: Vec<[f32; 3]> =
                    t.p.iter().map(|v| frame.havok_for_mc([v[0] as f64, v[1] as f64, v[2] as f64])).collect();
                let tri = Triangle {
                    origin: F32Vector4(h[0][0], h[0][1], h[0][2], 0.0),
                    edge1: F32Vector4(h[1][0] - h[0][0], h[1][1] - h[0][1], h[1][2] - h[0][2], 0.0),
                    edge2: F32Vector4(h[2][0] - h[0][0], h[2][1] - h[0][1], h[2][2] - h[0][2], 0.0),
                };
                ez.set_color(&F32Vector4(t.color[0], t.color[1], t.color[2], 1.0));
                ez.draw_triangle(&tri);
                drawn += 1;
                if drawn >= MAX_TRIS_PER_FRAME {
                    break 'outer;
                }
            }
        }
        if let Some((lo, hi)) = selection {
            ez.set_color(&F32Vector4(0.0, 0.0, 0.0, 1.0));
            let c = |x: f32, y: f32, z: f32| {
                let h = frame.havok_for_mc([x as f64, y as f64, z as f64]);
                HavokPosition(h[0], h[1], h[2], 0.0)
            };
            let xs = [lo[0], hi[0]];
            let ys = [lo[1], hi[1]];
            let zs = [lo[2], hi[2]];
            for &y in &ys {
                for &z in &zs {
                    ez.draw_line(&c(xs[0], y, z), &c(xs[1], y, z));
                }
                for &x in &xs {
                    ez.draw_line(&c(x, y, zs[0]), &c(x, y, zs[1]));
                }
            }
            for &x in &xs {
                for &z in &zs {
                    ez.draw_line(&c(x, ys[0], z), &c(x, ys[1], z));
                }
            }
        }
        drawn
    }
}
