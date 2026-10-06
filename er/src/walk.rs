//! Minecraft movement on Elden Ring's real collision (sheet: systems.movement, v0.2).
//!
//! Minecraft's physics numbers (walk/sprint/sneak speed, jump height, gravity and drag, 0.6-block
//! step, 0.6 x 1.8 player box) run here, every frame, against Elden Ring's own map collision through
//! Havok ray casts at the player's actual position, plus Minecraft's solid blocks. No copied height
//! field, so caves, bridges, stairs and roofs are what they are in Elden Ring.
//! Minecraft's player follows this position every tick (the Fabric side's follow mode), so its own
//! rules (fall damage, hunger, block placing, attacks) still apply.

use eldenring::cs::{CSPhysWorld, PlayerIns};
use eldenring::position::{HavokPosition, PositionDelta};

use crate::blocks::Blocks;
use crate::coords::Frame;

/// Map collision filter (CS2-in-ER field note: map geometry, not characters).
const RAY_FILTER: u32 = 0x0200_0058;

// Minecraft's numbers, in blocks (= metres) and seconds.
const WALK: f64 = 4.317;
const SPRINT: f64 = 5.612;
const SNEAK: f64 = 1.295;
const GROUND_RATE: f64 = 12.1; // 0.546 friction per tick
const AIR_RATE: f64 = 1.89; // 0.91 per tick
const AIR_ACCEL: f64 = 8.0; // 0.02 blocks/tick^2
const AIR_ACCEL_SPRINT: f64 = 10.4;
const GRAVITY: f64 = 31.7; // 0.08 per tick^2 with 0.98 drag
const DRAG: f64 = 0.404;
const JUMP: f64 = 8.9; // 1.25-block jump
const SPRINT_JUMP_BOOST: f64 = 4.0;
const STEP: f64 = 0.6;
const RADIUS: f64 = 0.3;
const HEIGHT: f64 = 1.8;
const SKIN: f64 = 0.04;
/// Heights above the feet where walls are probed (everything below is a step).
const WALL_HEIGHTS: [f64; 3] = [STEP + 0.05, 1.1, 1.7];
/// Walking down stairs and slopes: stay on the ground when it drops by up to this much.
const SNAP_DOWN: f64 = 0.55;
const MAX_DT: f64 = 0.05;

#[derive(Clone, Copy, Default, Debug)]
pub struct MoveInput {
    pub forward: f64, // +1 forward, -1 back
    pub strafe: f64,  // +1 right, -1 left
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
}

pub struct Walker {
    /// Feet, Minecraft coordinates.
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub on_ground: bool,
    pub sprinting: bool,
    pub active: bool,
    pub rays: u32,
    pub blocked: u32,
    pub airborne_since_ground: f64,
}

struct Ctx<'a> {
    world: &'a CSPhysWorld,
    frame: &'a Frame,
    player: &'a PlayerIns,
    blocks: &'a Blocks,
}

impl Ctx<'_> {
    /// Distance along `dir` (unit, MC space) from `from` to Elden Ring's map, within `len`.
    fn ray(&self, from: [f64; 3], dir: [f64; 3], len: f64, rays: &mut u32) -> Option<f64> {
        *rays += 1;
        let o = self.frame.havok_for_mc(from);
        // MC z is mirrored global z, and Havok follows global.
        let d = PositionDelta((dir[0] * len) as f32, (dir[1] * len) as f32, (-dir[2] * len) as f32);
        let hit = self.world.cast_ray(RAY_FILTER, &HavokPosition(o[0], o[1], o[2], 0.0), d, self.player)?;
        let h = self.frame.mc_for_havok([hit.0, hit.1, hit.2]);
        let dist = ((h[0] - from[0]).powi(2) + (h[1] - from[1]).powi(2) + (h[2] - from[2]).powi(2)).sqrt();
        Some(dist.min(len))
    }

    /// Highest Minecraft block top under the box, between `lo` and `hi` (feet heights).
    fn block_ground(&self, p: [f64; 3], lo: f64, hi: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        let (x0, x1) = ((p[0] - RADIUS + 0.001).floor() as i32, (p[0] + RADIUS - 0.001).floor() as i32);
        let (z0, z1) = ((p[2] - RADIUS + 0.001).floor() as i32, (p[2] + RADIUS - 0.001).floor() as i32);
        let (y0, y1) = ((lo - 1.0).floor() as i32, hi.floor() as i32);
        for x in x0..=x1 {
            for z in z0..=z1 {
                for y in y0..=y1 {
                    let top = (y + 1) as f64;
                    if top >= lo && top <= hi && self.blocks.is_solid(x, y, z) {
                        best = Some(best.map_or(top, |b: f64| b.max(top)));
                    }
                }
            }
        }
        best
    }

    /// Would the box at `p` overlap a Minecraft block?
    fn box_in_blocks(&self, p: [f64; 3]) -> bool {
        let (x0, x1) = ((p[0] - RADIUS + 0.001).floor() as i32, (p[0] + RADIUS - 0.001).floor() as i32);
        let (z0, z1) = ((p[2] - RADIUS + 0.001).floor() as i32, (p[2] + RADIUS - 0.001).floor() as i32);
        let (y0, y1) = ((p[1] + 0.001).floor() as i32, (p[1] + HEIGHT - 0.001).floor() as i32);
        for x in x0..=x1 {
            for z in z0..=z1 {
                for y in y0..=y1 {
                    if self.blocks.is_solid(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Walker {
    pub fn new() -> Self {
        Walker {
            pos: [0.0; 3],
            vel: [0.0; 3],
            on_ground: true,
            sprinting: false,
            active: false,
            rays: 0,
            blocked: 0,
            airborne_since_ground: 0.0,
        }
    }

    /// Start (or restart) from where Elden Ring has the player.
    pub fn reset(&mut self, feet: [f64; 3]) {
        self.pos = feet;
        self.vel = [0.0; 3];
        self.on_ground = true;
        self.sprinting = false;
        self.active = false;
    }

    /// One frame. `yaw`: Minecraft yaw (degrees) of the camera.
    pub fn step(
        &mut self,
        world: &CSPhysWorld,
        frame: &Frame,
        player: &PlayerIns,
        blocks: &Blocks,
        input: MoveInput,
        yaw: f32,
        dt: f64,
    ) {
        self.active = true;
        let dt = dt.clamp(0.0, MAX_DT);
        if dt <= 0.0 {
            return;
        }
        let cx = Ctx { world, frame, player, blocks };
        let mut rays = 0u32;

        // ---- wished direction (camera-relative, Minecraft axes) ----
        let yr = (yaw as f64).to_radians();
        let fwd = [-yr.sin(), 0.0, yr.cos()];
        let right = [-yr.cos(), 0.0, -yr.sin()];
        let (mut f, mut s) = (input.forward, input.strafe);
        let len = (f * f + s * s).sqrt();
        if len > 1.0 {
            f /= len;
            s /= len;
        }
        let wish = [fwd[0] * f + right[0] * s, fwd[2] * f + right[2] * s];
        if input.sprint && input.forward > 0.0 && !input.sneak {
            self.sprinting = true;
        }
        if input.forward <= 0.0 || input.sneak {
            self.sprinting = false;
        }
        let speed = if input.sneak {
            SNEAK
        } else if self.sprinting {
            SPRINT
        } else {
            WALK
        };

        // ---- horizontal velocity ----
        if self.on_ground {
            let k = (GROUND_RATE * dt).min(1.0);
            self.vel[0] += (wish[0] * speed - self.vel[0]) * k;
            self.vel[2] += (wish[1] * speed - self.vel[2]) * k;
        } else {
            let a = if self.sprinting { AIR_ACCEL_SPRINT } else { AIR_ACCEL };
            let decay = (-AIR_RATE * dt).exp();
            self.vel[0] = self.vel[0] * decay + wish[0] * a * dt;
            self.vel[2] = self.vel[2] * decay + wish[1] * a * dt;
        }

        // ---- jump ----
        if input.jump && self.on_ground {
            self.vel[1] = JUMP;
            self.on_ground = false;
            if self.sprinting {
                self.vel[0] += fwd[0] * SPRINT_JUMP_BOOST;
                self.vel[2] += fwd[2] * SPRINT_JUMP_BOOST;
            }
        }

        // ---- gravity ----
        if !self.on_ground {
            self.vel[1] += (-GRAVITY - DRAG * self.vel[1]) * dt;
        } else {
            self.vel[1] = 0.0;
        }

        // ---- horizontal move, one axis at a time (Minecraft resolves axes separately) ----
        let mut p = self.pos;
        for axis in [0usize, 2] {
            let d = self.vel[axis] * dt;
            if d.abs() < 1e-6 {
                continue;
            }
            let dir = if axis == 0 { [d.signum(), 0.0, 0.0] } else { [0.0, 0.0, d.signum()] };
            let side = if axis == 0 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
            let reach = RADIUS + d.abs() + SKIN;
            let mut allowed = d.abs();
            for h in WALL_HEIGHTS {
                for off in [0.0, -RADIUS * 0.8, RADIUS * 0.8] {
                    let from = [p[0] + side[0] * off, p[1] + h, p[2] + side[2] * off];
                    if let Some(hit) = cx.ray(from, dir, reach, &mut rays) {
                        allowed = allowed.min((hit - RADIUS - SKIN).max(0.0));
                    }
                }
            }
            let mut np = p;
            np[axis] += d.signum() * allowed;
            // Minecraft blocks the player placed: no walking into them (stepping onto one is fine).
            let mut lifted = np;
            if cx.box_in_blocks(np) {
                lifted[1] += STEP;
                let can_step = self.on_ground && !cx.box_in_blocks(lifted);
                if can_step {
                    if let Some(top) = cx.block_ground(np, np[1], np[1] + STEP) {
                        np[1] = top;
                    }
                } else {
                    np = p;
                    allowed = 0.0;
                }
            }
            // Sneaking: don't walk off an edge.
            if input.sneak && self.on_ground && allowed > 0.0 {
                let ground = self.ground(&cx, np, SNAP_DOWN, &mut rays);
                if ground.is_none() {
                    np = p;
                    allowed = 0.0;
                }
            }
            if allowed + 1e-6 < d.abs() {
                self.vel[axis] = 0.0;
                self.blocked += 1;
            }
            p = np;
        }

        // ---- vertical ----
        let dy = self.vel[1] * dt;
        if dy > 0.0 {
            // Head against a ceiling.
            let reach = HEIGHT - 1.0 + dy + SKIN;
            let mut room = dy;
            for off in [[0.0, 0.0], [RADIUS * 0.7, 0.0], [-RADIUS * 0.7, 0.0], [0.0, RADIUS * 0.7], [0.0, -RADIUS * 0.7]] {
                let from = [p[0] + off[0], p[1] + 1.0, p[2] + off[1]];
                if let Some(hit) = cx.ray(from, [0.0, 1.0, 0.0], reach, &mut rays) {
                    room = room.min((hit - (HEIGHT - 1.0) - SKIN).max(0.0));
                }
            }
            if cx.box_in_blocks([p[0], p[1] + room, p[2]]) {
                room = 0.0;
            }
            if room < dy {
                self.vel[1] = 0.0;
            }
            p[1] += room;
            self.on_ground = false;
        } else {
            // Falling, standing, or walking: find the ground under the box.
            let fall = -dy;
            let snap = if self.on_ground { SNAP_DOWN } else { 0.0 };
            let ground = self.ground(&cx, p, fall + snap, &mut rays);
            match ground {
                Some(g) if g >= p[1] - fall - snap - 1e-4 => {
                    if !self.on_ground {
                        self.airborne_since_ground = 0.0;
                    }
                    p[1] = g;
                    self.vel[1] = 0.0;
                    self.on_ground = true;
                }
                _ => {
                    p[1] -= fall;
                    self.on_ground = false;
                }
            }
        }
        if !self.on_ground {
            self.airborne_since_ground += dt;
        }
        self.pos = p;
        self.rays = rays;
    }

    /// The highest walkable surface under the box: from a step above the feet down to `below` under
    /// them. Elden Ring's map (centre and four corners) and Minecraft blocks.
    fn ground(&self, cx: &Ctx, p: [f64; 3], below: f64, rays: &mut u32) -> Option<f64> {
        let top = p[1] + STEP;
        let len = STEP + below + 0.02;
        let mut best: Option<f64> = None;
        let c = RADIUS * 0.75;
        for off in [[0.0, 0.0], [c, c], [c, -c], [-c, c], [-c, -c]] {
            let from = [p[0] + off[0], top, p[2] + off[1]];
            if let Some(hit) = cx.ray(from, [0.0, -1.0, 0.0], len, rays) {
                let g = top - hit;
                best = Some(best.map_or(g, |b: f64| b.max(g)));
            }
        }
        if let Some(b) = cx.block_ground(p, p[1] - below - 0.02, top) {
            best = Some(best.map_or(b, |g: f64| g.max(b)));
        }
        best
    }
}
