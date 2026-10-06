//! Minecraft's HUD (hotbar, hearts, hunger, crosshair, chat, inventory screens) shown over Elden Ring
//! (sheet: systems.hud_overlay, v0.2).
//!
//! Minecraft renders its HUD into the shared overlay frame. Here that frame becomes flat rectangles
//! (one per GUI pixel, runs of equal colour merged) in screen pixels; overlay.rs draws them on top of
//! the finished Elden Ring frame (Dear ImGui through hudhook), so the HUD is pinned to the screen,
//! pixel-sharp, and never moves with the camera.

use std::time::{Duration, Instant};

use crate::link::Link;
use crate::overlay;

/// ImGui's 16-bit indices: 4 vertices per rectangle, under 65536 per draw list.
const MAX_QUADS: usize = 16000;
const REBUILD_EVERY: Duration = Duration::from_millis(50);
pub use overlay::Quad;

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
            let mut run: Option<(u32, [u8; 4])> = None; // start x, colour
            while x <= w {
                let c = if x < w {
                    let i = ((row * w + x) * 4) as usize;
                    if px[i + 3] >= 24 {
                        Some([px[i] & 0xFC, px[i + 1] & 0xFC, px[i + 2] & 0xFC, px[i + 3] & 0xF0])
                    } else {
                        None
                    }
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
                                // ImGui colour: 0xAABBGGRR.
                                color: u32::from_le_bytes([rc[0], rc[1], rc[2], rc[3] | 0x0F]),
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

    /// Hand the newest HUD to the screen overlay (`visible`: Minecraft is driving).
    pub fn publish(&self, visible: bool) {
        overlay::publish(visible, self.w, self.h, &self.quads, self.frames);
    }
}
