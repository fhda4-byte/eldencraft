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
const MAX_TRIS_PER_FRAME: usize = 24000;
const DRAIN_BYTES_PER_FRAME: u64 = 16 << 20;

struct Tri {
    p: [[f32; 3]; 3], // absolute Minecraft coords
    centre: [f32; 3],
    color: [f32; 4],
}

struct Image {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Image {
    fn texel(&self, u: f32, v: f32) -> [f32; 4] {
        if self.w == 0 {
            return [0.6, 0.6, 0.6, 1.0];
        }
        let x = (u.clamp(0.0, 0.9999) * self.w as f32) as usize;
        let y = (v.clamp(0.0, 0.9999) * self.h as f32) as usize;
        let i = (y * self.w as usize + x) * 4;
        if i + 3 >= self.px.len() {
            return [0.6, 0.6, 0.6, 1.0];
        }
        [self.px[i] as f32 / 255.0, self.px[i + 1] as f32 / 255.0, self.px[i + 2] as f32 / 255.0, self.px[i + 3] as f32 / 255.0]
    }
}

/// One avatar triangle: positions relative to the player's feet (blocks, MC axes).
struct AvatarTri {
    p: [[f32; 3]; 3],
    color: [f32; 4],
}

pub struct Blocks {
    textures: HashMap<u32, Image>,
    avatar: Vec<AvatarTri>,
    pub avatar_frames: u64,
    atlas_w: u32,
    atlas_h: u32,
    atlas: Vec<u8>,
    sections: HashMap<(i32, i32, i32), Vec<Tri>>,
    /// Which blocks of each section are solid (bit x + 16z + 256y), for the movement controller.
    solids: HashMap<(i32, i32, i32), Vec<u8>>,
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

/// Splits a textured triangle into one flat-coloured quad per texel (or per `step` x `step` texels),
/// so a flat-colour debug renderer still shows the texture: Steve's face, armour, block patterns.
/// `uv` in 0..1; `tex` returns RGBA for a texel (x, y). Cells whose centre is outside the triangle are
/// left to the neighbouring triangle of the quad; transparent texels (alpha < 0.5) are skipped.
fn texelize(
    pos: [[f32; 3]; 3],
    uv: [[f32; 2]; 3],
    size: (u32, u32),
    max_cells: usize,
    tint: [f32; 4],
    shade: f32,
    tex: &dyn Fn(u32, u32) -> [f32; 4],
    out: &mut Vec<([[f32; 3]; 3], [f32; 4])>,
) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let t: Vec<[f32; 2]> = uv.iter().map(|q| [q[0] * w, q[1] * h]).collect();
    let (e1, e2) = ([t[1][0] - t[0][0], t[1][1] - t[0][1]], [t[2][0] - t[0][0], t[2][1] - t[0][1]]);
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let flat = |out: &mut Vec<([[f32; 3]; 3], [f32; 4])>| {
        let c = [
            (t[0][0] + t[1][0] + t[2][0]) / 3.0,
            (t[0][1] + t[1][1] + t[2][1]) / 3.0,
        ];
        let px = tex(c[0].max(0.0) as u32, c[1].max(0.0) as u32);
        if px[3] >= 0.5 {
            out.push((pos, [px[0] * tint[0] * shade, px[1] * tint[1] * shade, px[2] * tint[2] * shade, 1.0]));
        }
    };
    if det.abs() < 1e-6 {
        flat(out);
        return;
    }
    // texel space -> barycentric (b1, b2) -> position
    let to_bary = |x: f32, y: f32| {
        let (dx, dy) = (x - t[0][0], y - t[0][1]);
        ((dx * e2[1] - dy * e2[0]) / det, (e1[0] * dy - e1[1] * dx) / det)
    };
    let at = |b1: f32, b2: f32| {
        [
            pos[0][0] + b1 * (pos[1][0] - pos[0][0]) + b2 * (pos[2][0] - pos[0][0]),
            pos[0][1] + b1 * (pos[1][1] - pos[0][1]) + b2 * (pos[2][1] - pos[0][1]),
            pos[0][2] + b1 * (pos[1][2] - pos[0][2]) + b2 * (pos[2][2] - pos[0][2]),
        ]
    };
    let minx = t.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min).floor();
    let maxx = t.iter().map(|q| q[0]).fold(f32::NEG_INFINITY, f32::max).ceil();
    let miny = t.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min).floor();
    let maxy = t.iter().map(|q| q[1]).fold(f32::NEG_INFINITY, f32::max).ceil();
    let cells = ((maxx - minx) * (maxy - miny)).max(1.0) as usize;
    let step = ((cells as f32 / max_cells as f32).sqrt().ceil()).max(1.0);
    let mut y = miny;
    while y < maxy {
        let mut x = minx;
        while x < maxx {
            let (cx, cy) = (x + step * 0.5, y + step * 0.5);
            let (b1, b2) = to_bary(cx, cy);
            if b1 >= -1e-4 && b2 >= -1e-4 && b1 + b2 <= 1.0 + 1e-4 {
                let px = tex(cx.max(0.0) as u32, cy.max(0.0) as u32);
                if px[3] >= 0.5 {
                    let color = [px[0] * tint[0] * shade, px[1] * tint[1] * shade, px[2] * tint[2] * shade, 1.0];
                    let c00 = { let (a, b) = to_bary(x, y); at(a, b) };
                    let c10 = { let (a, b) = to_bary(x + step, y); at(a, b) };
                    let c01 = { let (a, b) = to_bary(x, y + step); at(a, b) };
                    let c11 = { let (a, b) = to_bary(x + step, y + step); at(a, b) };
                    out.push(([c00, c10, c11], color));
                    out.push(([c00, c11, c01], color));
                }
            }
            x += step;
        }
        y += step;
    }
}

impl Image {
    fn px(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.w || y >= self.h {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let i = ((y * self.w + x) * 4) as usize;
        [self.px[i] as f32 / 255.0, self.px[i + 1] as f32 / 255.0, self.px[i + 2] as f32 / 255.0, self.px[i + 3] as f32 / 255.0]
    }
}

impl Blocks {
    fn atlas_px(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.atlas_w || y >= self.atlas_h {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let i = ((y * self.atlas_w + x) * 4) as usize;
        [self.atlas[i] as f32 / 255.0, self.atlas[i + 1] as f32 / 255.0, self.atlas[i + 2] as f32 / 255.0, self.atlas[i + 3] as f32 / 255.0]
    }
}

impl Blocks {
    pub fn new() -> Self {
        Blocks {
            textures: HashMap::new(),
            avatar: Vec::new(),
            avatar_frames: 0,
            atlas_w: 0,
            atlas_h: 0,
            atlas: Vec::new(),
            sections: HashMap::new(),
            solids: HashMap::new(),
            messages: 0,
        }
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
            if matches!(kind, REN_ATLAS | REN_SECTION | REN_CLEAR_ALL | REN_ATLAS_REGION | REN_TEXTURE | REN_AVATAR | REN_SOLIDS) {
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
                REN_CLEAR_ALL => {
                    self.sections.clear();
                    self.solids.clear();
                }
                REN_SOLIDS => {
                    let (Some(sx), Some(sy), Some(sz), Some(count)) =
                        (read::<i32>(&p, 0), read::<i32>(&p, 4), read::<i32>(&p, 8), read::<u32>(&p, 12))
                    else {
                        continue;
                    };
                    if count == 0 || p.len() < 16 + 512 {
                        self.solids.remove(&(sx, sy, sz));
                    } else {
                        self.solids.insert((sx, sy, sz), p[16..16 + 512].to_vec());
                    }
                }
                REN_TEXTURE => {
                    let Some(t) = read::<RenTexture>(&p, 0) else { continue };
                    let bytes = t.width as usize * t.height as usize * 4;
                    let base = size_of::<RenTexture>();
                    if p.len() >= base + bytes {
                        self.textures.insert(t.id, Image { w: t.width, h: t.height, px: p[base..base + bytes].to_vec() });
                        crate::log!("texture {} {}x{}", t.id, t.width, t.height);
                    }
                }
                REN_AVATAR => {
                    let Some(a) = read::<RenAvatar>(&p, 0) else { continue };
                    self.avatar_frames += 1;
                    self.avatar.clear();
                    let batches = size_of::<RenAvatar>();
                    let verts = batches + a.batch_count as usize * size_of::<RenBatch>();
                    for b in 0..a.batch_count as usize {
                        let Some(batch) = read::<RenBatch>(&p, batches + b * size_of::<RenBatch>()) else { break };
                        for t in 0..(batch.count as usize / 3) {
                            let mut vs = [[0f32; 3]; 3];
                            let mut uvs = [[0f32; 2]; 3];
                            let mut tint = [0f32; 4];
                            let mut ok = true;
                            for k in 0..3 {
                                let idx = batch.first as usize + t * 3 + k;
                                let Some(vx) = read::<RenVertex>(&p, verts + idx * size_of::<RenVertex>()) else {
                                    ok = false;
                                    break;
                                };
                                vs[k] = [vx.x, vx.y, vx.z];
                                uvs[k] = [vx.u, vx.v];
                                let c = rgba(vx.color);
                                for i in 0..4 {
                                    tint[i] += c[i] / 3.0;
                                }
                            }
                            if !ok {
                                break;
                            }
                            let mut cells = Vec::new();
                            if batch.texture == 0 {
                                texelize(vs, uvs, (self.atlas_w, self.atlas_h), 64, tint, 1.0, &|x, y| self.atlas_px(x, y), &mut cells);
                            } else if let Some(img) = self.textures.get(&batch.texture) {
                                texelize(vs, uvs, (img.w, img.h), 64, tint, 1.0, &|x, y| img.px(x, y), &mut cells);
                            }
                            for (p3, color) in cells {
                                self.avatar.push(AvatarTri { p: p3, color });
                            }
                        }
                    }
                }
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
                        let mut uvs = [[0f32; 2]; 3];
                        let mut tint = [0f32; 4];
                        let mut flags = 0u32;
                        let mut ok = true;
                        for k in 0..3 {
                            let Some(vx) = read::<RenVertex>(&p, base + (t * 3 + k) * size_of::<RenVertex>()) else {
                                ok = false;
                                break;
                            };
                            vs[k] = [origin[0] + vx.x, origin[1] + vx.y, origin[2] + vx.z];
                            uvs[k] = [vx.u, vx.v];
                            let c = rgba(vx.color);
                            for i in 0..4 {
                                tint[i] += c[i] / 3.0;
                            }
                            flags = vx.flags;
                        }
                        if !ok {
                            break;
                        }
                        // Minecraft's fixed face shading: down, up, north, south, west, east.
                        let shade = match (flags >> 4) & 7 {
                            1 => 0.5,
                            2 => 1.0,
                            3 | 4 => 0.8,
                            5 | 6 => 0.6,
                            _ => 0.9,
                        };
                        // 4 x 4 cells per block face: the texture's pattern without too many draws.
                        let mut cells = Vec::new();
                        texelize(vs, uvs, (self.atlas_w, self.atlas_h), 8, tint, shade, &|x, y| self.atlas_px(x, y), &mut cells);
                        for (p3, color) in cells {
                            let centre = [
                                (p3[0][0] + p3[1][0] + p3[2][0]) / 3.0,
                                (p3[0][1] + p3[1][1] + p3[2][1]) / 3.0,
                                (p3[0][2] + p3[1][2] + p3[2][2]) / 3.0,
                            ];
                            tris.push(Tri { p: p3, centre, color });
                        }
                    }
                    self.sections.insert(key, tris);
                }
                _ => {}
            }
        }
    }

    /// Is the Minecraft block at (x, y, z) solid (placed by the player)?
    pub fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        let key = (x.div_euclid(16), y.div_euclid(16), z.div_euclid(16));
        let Some(bits) = self.solids.get(&key) else { return false };
        let (lx, ly, lz) = (x.rem_euclid(16) as usize, y.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
        let bit = lx + 16 * lz + 256 * ly;
        bits[bit / 8] & (1 << (bit % 8)) != 0
    }

    pub fn has_avatar(&self) -> bool {
        !self.avatar.is_empty()
    }

    /// The Minecraft player model at its feet (MC coords). Returns triangles drawn.
    pub fn draw_avatar(&self, ez: &mut CSEzDraw, frame: &Frame, feet: [f64; 3]) -> usize {
        ez.set_fill_mode(EzDrawFillMode::Fill);
        for t in &self.avatar {
            let h: Vec<[f32; 3]> = t
                .p
                .iter()
                .map(|v| frame.havok_for_mc([feet[0] + v[0] as f64, feet[1] + v[1] as f64, feet[2] + v[2] as f64]))
                .collect();
            let tri = Triangle {
                origin: F32Vector4(h[0][0], h[0][1], h[0][2], 0.0),
                edge1: F32Vector4(h[1][0] - h[0][0], h[1][1] - h[0][1], h[1][2] - h[0][2], 0.0),
                edge2: F32Vector4(h[2][0] - h[0][0], h[2][1] - h[0][1], h[2][2] - h[0][2], 0.0),
            };
            ez.set_color(&F32Vector4(t.color[0], t.color[1], t.color[2], 1.0));
            ez.draw_triangle(&tri);
        }
        self.avatar.len()
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
