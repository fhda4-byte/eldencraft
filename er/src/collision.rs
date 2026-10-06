//! Elden Ring's ground -> Minecraft collision (sheet: systems.collision).
//!
//! First version: a height field from Havok ray casts straight down, one per block corner, for
//! 8x8-block columns around the player. Each column becomes Minecraft collision regions (8^3
//! cubes): exact triangles for the player's smooth collider plus 1/8-block voxels so Minecraft
//! knows the region (it holds the player until the ground under them is known).
//! Misses: walls, overhangs, building interiors (one hit per column).

use std::collections::{HashSet, VecDeque};

use eldenring::cs::{CSHavokMan, CSPhysWorld, PlayerIns};
use eldenring::position::{HavokPosition, PositionDelta};
use fromsoftware_shared::FromStatic;

use crate::coords::Frame;
use crate::link::Link;
use crate::log;
use crate::proto::*;

/// Map collision filter (CS2-in-ER field note: hits map collision, not characters).
const RAY_FILTER: u32 = 0x0200_0058;
const RADIUS_REGIONS: i32 = 3;
const COLUMNS_PER_FRAME: usize = 2;
/// Ray starts above the player's feet, lowest first: the first surface found from just above the
/// player's level wins, so floors under a roof (castles, caves) are found before the roof.
const RAY_STARTS: [f64; 4] = [2.0, 8.0, 20.0, 45.0];
const RAY_BELOW: f64 = 60.0; // how far under the feet a ray still looks

/// The walkable surface at Minecraft column (x, z) nearest above-or-below the player's level.
pub fn ground_at(world: &CSPhysWorld, frame: &Frame, player: &PlayerIns, x: f64, z: f64, feet_y: f64) -> Option<f64> {
    for top in RAY_STARTS {
        let o = frame.havok_for_mc([x, feet_y + top, z]);
        let origin = HavokPosition(o[0], o[1], o[2], 0.0);
        let len = (top + RAY_BELOW) as f32;
        if let Some(hit) = world.cast_ray(RAY_FILTER, &origin, PositionDelta(0.0, -len, 0.0), player) {
            return Some(frame.mc_for_havok([hit.0, hit.1, hit.2])[1]);
        }
    }
    None
}

/// Ground under a point, searching down from `from_y` (MC coords).
pub fn ground_below(frame: &Frame, player: &PlayerIns, x: f64, z: f64, from_y: f64, depth: f64) -> Option<f64> {
    let havok = unsafe { CSHavokMan::instance() }.ok()?;
    let o = frame.havok_for_mc([x, from_y, z]);
    let origin = HavokPosition(o[0], o[1], o[2], 0.0);
    havok
        .phys_world
        .cast_ray(RAY_FILTER, &origin, PositionDelta(0.0, -(depth as f32), 0.0), player)
        .map(|hit| frame.mc_for_havok([hit.0, hit.1, hit.2])[1])
}

pub struct Collision {
    pub epoch: u32,
    /// Last ground height found under the player (MC y): fills columns where no ray hits.
    pub fallback_y: Option<f64>,
    done: HashSet<(i32, i32)>,
    pending: VecDeque<(u32, Vec<u8>)>,
    pub columns_sent: u32,
    pub rays_hit: u64,
    pub rays_missed: u64,
}

fn bytes_of<T: Copy>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, size_of::<T>()) }
}

fn push_bytes<T: Copy>(out: &mut Vec<u8>, v: &T) {
    out.extend_from_slice(bytes_of(v));
}

impl Collision {
    pub fn new() -> Self {
        Collision {
            epoch: 1,
            fallback_y: None,
            done: HashSet::new(),
            pending: VecDeque::new(),
            columns_sent: 0,
            rays_hit: 0,
            rays_missed: 0,
        }
    }

    /// World change or teleport: Minecraft drops everything and we stream again.
    pub fn reset(&mut self, link: &Link) {
        self.epoch += 1;
        self.done.clear();
        self.pending.clear();
        let mut p = Vec::new();
        push_bytes(&mut p, &self.epoch);
        self.pending.push_back((COL_CLEAR, p));
        self.flush(link);
        log!("collision: new epoch {}", self.epoch);
    }

    fn flush(&mut self, link: &Link) -> bool {
        while let Some((kind, payload)) = self.pending.front() {
            if !link.write_collision(*kind, payload) {
                return false;
            }
            self.pending.pop_front();
        }
        true
    }

    /// Stream the columns around this point again (after the player fell through a gap).
    pub fn redo_around(&mut self, mc: [f64; 3]) {
        let cx = (mc[0] / REGION_SIZE as f64).floor() as i32;
        let cz = (mc[2] / REGION_SIZE as f64).floor() as i32;
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.done.remove(&(cx + dx, cz + dz));
            }
        }
    }

    /// `mc_feet`: the player's feet in Minecraft coords.
    pub fn step(&mut self, link: &Link, frame: &Frame, player: &PlayerIns, mc_feet: [f64; 3]) {
        if !self.flush(link) {
            return;
        }
        let cx = (mc_feet[0] / REGION_SIZE as f64).floor() as i32;
        let cz = (mc_feet[2] / REGION_SIZE as f64).floor() as i32;
        let mut todo: Vec<(i32, i32)> = Vec::new();
        for dz in -RADIUS_REGIONS..=RADIUS_REGIONS {
            for dx in -RADIUS_REGIONS..=RADIUS_REGIONS {
                let key = (cx + dx, cz + dz);
                if !self.done.contains(&key) {
                    todo.push(key);
                }
            }
        }
        todo.sort_by_key(|&(x, z)| (x - cx).pow(2) + (z - cz).pow(2));
        let Ok(havok) = (unsafe { CSHavokMan::instance() }) else {
            return;
        };
        let world = &*havok.phys_world;
        for &(rx, rz) in todo.iter().take(COLUMNS_PER_FRAME) {
            self.build_column(world, frame, player, rx, rz, mc_feet[1]);
            self.done.insert((rx, rz));
            self.columns_sent += 1;
            if !self.flush(link) {
                break;
            }
        }
    }

    fn build_column(
        &mut self,
        world: &CSPhysWorld,
        frame: &Frame,
        player: &PlayerIns,
        rx: i32,
        rz: i32,
        feet_y: f64,
    ) {
        let x0 = rx * REGION_SIZE;
        let z0 = rz * REGION_SIZE;
        let n = (REGION_SIZE + 1) as usize;
        // Heights (MC y) at block corners (x0 + i, z0 + j); NaN = no ground found.
        let mut h = vec![f64::NAN; n * n];
        let mut hits = 0usize;
        for j in 0..n {
            for i in 0..n {
                let (x, z) = ((x0 + i as i32) as f64, (z0 + j as i32) as f64);
                if let Some(y) = ground_at(world, frame, player, x, z, feet_y) {
                    h[j * n + i] = y;
                    hits += 1;
                    self.rays_hit += 1;
                } else {
                    self.rays_missed += 1;
                }
            }
        }
        // Holes (no surface found: inside walls, outside loaded collision) take the nearest height
        // found in this column, or the last ground found under the player: never an open hole the
        // Minecraft player could fall through.
        let fallback = self.fallback_y.unwrap_or(feet_y);
        if hits == 0 {
            h.iter_mut().for_each(|v| *v = fallback);
        } else if hits < n * n {
            let known: Vec<(usize, f64)> = h.iter().enumerate().filter(|(_, v)| !v.is_nan()).map(|(k, v)| (k, *v)).collect();
            for k in 0..n * n {
                if h[k].is_nan() {
                    let (ki, kj) = ((k % n) as i64, (k / n) as i64);
                    let best = known
                        .iter()
                        .min_by_key(|(q, _)| ((*q % n) as i64 - ki).pow(2) + ((*q / n) as i64 - kj).pow(2))
                        .map(|(_, v)| *v)
                        .unwrap_or(fallback);
                    h[k] = best;
                }
            }
        }
        if self.columns_sent < 6 {
            log!(
                "collision column ({rx}, {rz}): {hits}/{} rays hit, feet y {:.2}, heights {:.2}..{:.2}",
                n * n,
                feet_y,
                h.iter().cloned().fold(f64::INFINITY, f64::min),
                h.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            );
        }
        let at = |i: usize, j: usize| h[j * n + i];

        // Triangles, two per block cell, winding with the normal up (out of the ground).
        let mut tris: Vec<ColTri> = Vec::new();
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let (h00, h10, h01, h11) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
                if h00.is_nan() || h10.is_nan() || h01.is_nan() || h11.is_nan() {
                    continue;
                }
                for v in [h00, h10, h01, h11] {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                let (x, z) = ((x0 + i as i32) as f32, (z0 + j as i32) as f32);
                tris.push(ColTri {
                    v: [x, h00 as f32, z, x, h01 as f32, z + 1.0, x + 1.0, h10 as f32, z],
                    flags: TRI_TERRAIN,
                });
                tris.push(ColTri {
                    v: [x + 1.0, h10 as f32, z, x, h01 as f32, z + 1.0, x + 1.0, h11 as f32, z + 1.0],
                    flags: TRI_TERRAIN,
                });
            }
        }
        if !lo.is_finite() {
            // No ground here: still tell Minecraft the regions around the player's height are known (empty).
            lo = feet_y;
            hi = feet_y;
        }
        let ry_lo = ((lo - 16.0) / REGION_SIZE as f64).floor() as i32;
        let ry_hi = ((hi + 16.0) / REGION_SIZE as f64).floor() as i32;

        let bilinear = |px: f64, pz: f64| -> f64 {
            let fx = px - x0 as f64;
            let fz = pz - z0 as f64;
            let i = (fx.floor() as i64).clamp(0, REGION_SIZE as i64 - 1) as usize;
            let j = (fz.floor() as i64).clamp(0, REGION_SIZE as i64 - 1) as usize;
            let (tx, tz) = (fx - i as f64, fz - j as f64);
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            if a.is_nan() || b.is_nan() || c.is_nan() || d.is_nan() {
                return f64::NAN;
            }
            (a * (1.0 - tx) + b * tx) * (1.0 - tz) + (c * (1.0 - tx) + d * tx) * tz
        };
        // Surface height per sub-voxel column (64 x 64 per region column).
        let sub = (REGION_SIZE * 8) as usize;
        let mut surf = vec![f64::NAN; sub * sub];
        for sz in 0..sub {
            for sx in 0..sub {
                surf[sz * sub + sx] = bilinear(x0 as f64 + (sx as f64 + 0.5) / 8.0, z0 as f64 + (sz as f64 + 0.5) / 8.0);
            }
        }

        let epoch = self.epoch;
        for ry in ry_lo..=ry_hi {
            let y0 = ry * REGION_SIZE;
            let region = |count: u32| ColRegion {
                min_x: x0,
                min_y: y0,
                min_z: z0,
                max_x: x0 + REGION_SIZE - 1,
                max_y: y0 + REGION_SIZE - 1,
                max_z: z0 + REGION_SIZE - 1,
                epoch,
                count,
            };
            // Triangles whose centre lies in this region.
            let mine: Vec<&ColTri> = tris
                .iter()
                .filter(|t| {
                    let cy = (t.v[1] + t.v[4] + t.v[7]) / 3.0;
                    (cy / REGION_SIZE as f32).floor() as i32 == ry
                })
                .collect();
            let mut p = Vec::with_capacity(32 + mine.len() * 40);
            push_bytes(&mut p, &region(mine.len() as u32));
            for t in &mine {
                push_bytes(&mut p, *t);
            }
            self.pending.push_back((COL_TRIS, p));

            // Voxels: everything under the surface is solid.
            let mut blocks: Vec<ColBlock> = Vec::new();
            for bz in 0..REGION_SIZE {
                for by in 0..REGION_SIZE {
                    for bx in 0..REGION_SIZE {
                        let mut bits = [0u64; 8];
                        let mut any = false;
                        for sy in 0..8 {
                            let y = (y0 + by) as f64 + (sy as f64 + 0.5) / 8.0;
                            for szz in 0..8 {
                                for sxx in 0..8 {
                                    let s = surf[(bz as usize * 8 + szz) * sub + bx as usize * 8 + sxx];
                                    if !s.is_nan() && y < s {
                                        bits[sy] |= 1u64 << (szz * 8 + sxx);
                                        any = true;
                                    }
                                }
                            }
                        }
                        if any {
                            blocks.push(ColBlock { x: x0 + bx, y: y0 + by, z: z0 + bz, pad: 0, bits });
                        }
                    }
                }
            }
            let mut p = Vec::with_capacity(32 + blocks.len() * 80);
            push_bytes(&mut p, &region(blocks.len() as u32));
            for b in &blocks {
                push_bytes(&mut p, b);
            }
            self.pending.push_back((COL_REGION, p));
        }
    }
}
