//! Minecraft's HUD (hotbar, hearts, hunger, crosshair, chat, inventory screens) shown over Elden Ring
//! (sheet: systems.hud_overlay, first version).
//!
//! Minecraft renders its HUD into the shared overlay frame. This version turns that frame into
//! flat-coloured quads (one per GUI pixel, runs of equal colour merged) and draws them with the debug
//! renderer on a plane just in front of Elden Ring's camera, so they cover the screen like a HUD.

use std::time::{Duration, Instant};

use eldenring::cs::CSEzDraw;
use eldenring::cs::EzDrawFillMode;
use fromsoftware_shared::{F32Vector4, Triangle};

use crate::link::Link;

const MAX_QUADS: usize = 16000;
const REBUILD_EVERY: Duration = Duration::from_millis(50);
/// Distance of the HUD plane in front of the camera (metres): past the near plane, before the world.
const PLANE: f32 = 0.3;

struct Quad {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 4],
}

pub struct Hud {
    quads: Vec<Quad>,
    w: f32,
    h: f32,
    last_build: Option<Instant>,
    pub frames: u64,
}

impl Hud {
    pub fn new() -> Self {
        Hud { quads: Vec::new(), w: 1920.0, h: 1080.0, last_build: None, frames: 0 }
    }

    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }

    /// Pick up Minecraft's newest HUD frame; `gui_scale` = Minecraft pixels per GUI pixel.
    pub fn update(&mut self, link: &Link, gui_scale: u32) {
        if self.last_build.map_or(false, |t| t.elapsed() < REBUILD_EVERY) {
            return;
        }
        if !link.acquire_overlay() {
            return;
        }
        self.last_build = Some(Instant::now());
        self.frames += 1;
        let (w, h, bottom_up, px) = link.overlay();
        if w == 0 || h == 0 || px.len() < (w * h * 4) as usize {
            return;
        }
        self.w = w as f32;
        self.h = h as f32;
        let step = gui_scale.clamp(1, 8);
        self.quads.clear();
        let mut y = 0;
        'rows: while y < h {
            let row = if bottom_up { h - 1 - y } else { y };
            let mut x = 0;
            let mut run: Option<(u32, [u8; 3])> = None; // start x, colour
            while x <= w {
                let c = if x < w {
                    let i = ((row * w + x) * 4) as usize;
                    if px[i + 3] >= 128 { Some([px[i] & 0xF8, px[i + 1] & 0xF8, px[i + 2] & 0xF8]) } else { None }
                } else {
                    None
                };
                match (run, c) {
                    (Some((_, rc)), Some(cc)) if rc == cc => {}
                    _ => {
                        if let Some((start, rc)) = run.take() {
                            self.quads.push(Quad {
                                x0: start as f32,
                                y0: y as f32,
                                x1: x as f32,
                                y1: (y + step) as f32,
                                color: [rc[0] as f32 / 255.0, rc[1] as f32 / 255.0, rc[2] as f32 / 255.0, 1.0],
                            });
                            if self.quads.len() >= MAX_QUADS {
                                break 'rows;
                            }
                        }
                        if let Some(cc) = c {
                            run = Some((x, cc));
                        }
                    }
                }
                x += step;
            }
            y += step;
        }
    }

    /// Draw on a plane in front of the camera. `cam`: camera-to-world rows (right, up, forward, position)
    /// in Havok space; `fov`: vertical field of view (radians).
    pub fn draw(&self, ez: &mut CSEzDraw, cam: [[f32; 3]; 4], fov: f32, aspect: f32) -> usize {
        if self.quads.is_empty() {
            return 0;
        }
        ez.set_fill_mode(EzDrawFillMode::Fill);
        let hh = PLANE * (fov * 0.5).tan();
        let hw = hh * aspect;
        let [right, up, fwd, pos] = cam;
        let at = |sx: f32, sy: f32| -> [f32; 3] {
            let u = (sx / self.w) * 2.0 - 1.0;
            let v = 1.0 - (sy / self.h) * 2.0;
            [
                pos[0] + fwd[0] * PLANE + right[0] * u * hw + up[0] * v * hh,
                pos[1] + fwd[1] * PLANE + right[1] * u * hw + up[1] * v * hh,
                pos[2] + fwd[2] * PLANE + right[2] * u * hw + up[2] * v * hh,
            ]
        };
        for q in &self.quads {
            let (a, b, c, d) = (at(q.x0, q.y0), at(q.x1, q.y0), at(q.x1, q.y1), at(q.x0, q.y1));
            ez.set_color(&F32Vector4(q.color[0], q.color[1], q.color[2], 1.0));
            for (p0, p1, p2) in [(a, b, c), (a, c, d)] {
                ez.draw_triangle(&Triangle {
                    origin: F32Vector4(p0[0], p0[1], p0[2], 0.0),
                    edge1: F32Vector4(p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2], 0.0),
                    edge2: F32Vector4(p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2], 0.0),
                });
            }
        }
        self.quads.len()
    }
}
