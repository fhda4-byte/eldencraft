//! Elden Ring <-> Minecraft coordinates (sheet: constants.axis_map / metres_per_block).
//!
//! "Global" is a stable Elden Ring world position in metres: overworld tiles m60/m61_XX_YY_00 are
//! 256 m squares, so global = tile * 256 + block-local position. It survives Havok re-basing and
//! game restarts, so blocks placed in Minecraft stay put. Other maps (legacy dungeons, catacombs)
//! use their block-local position and get their own world id.
//! Minecraft space = global / METRES_PER_BLOCK with Z mirrored (Elden Ring is left-handed,
//! Minecraft right-handed): to be confirmed in game (strafing A must move left).

use eldenring::cs::BlockId;

pub const METRES_PER_BLOCK: f64 = 1.0;
pub const MIRROR_Z: bool = true;
pub const TILE_METRES: f64 = 256.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Player's Havok position this frame.
    pub havok: [f32; 3],
    /// Same point in global metres.
    pub global: [f64; 3],
    pub world_id: u32,
}

pub fn world_and_global(block: BlockId, local: [f32; 3]) -> (u32, [f64; 3]) {
    let area = block.area();
    if area == 60 || area == 61 {
        let gx = block.block() as f64 * TILE_METRES + local[0] as f64;
        let gz = block.region() as f64 * TILE_METRES + local[2] as f64;
        ((area as u32) << 24, [gx, local[1] as f64, gz])
    } else {
        (block.0 as u32, [local[0] as f64, local[1] as f64, local[2] as f64])
    }
}

pub fn mc_from_global(g: [f64; 3]) -> [f64; 3] {
    let z = if MIRROR_Z { -g[2] } else { g[2] };
    [g[0] / METRES_PER_BLOCK, g[1] / METRES_PER_BLOCK, z / METRES_PER_BLOCK]
}

pub fn global_from_mc(m: [f64; 3]) -> [f64; 3] {
    let z = if MIRROR_Z { -m[2] } else { m[2] };
    [m[0] * METRES_PER_BLOCK, m[1] * METRES_PER_BLOCK, z * METRES_PER_BLOCK]
}

/// A direction in Elden Ring world space as a Minecraft direction (no scaling).
pub fn mc_dir(d: [f32; 3]) -> [f32; 3] {
    [d[0], d[1], if MIRROR_Z { -d[2] } else { d[2] }]
}

impl Frame {
    pub fn havok_for_global(&self, g: [f64; 3]) -> [f32; 3] {
        [
            self.havok[0] + (g[0] - self.global[0]) as f32,
            self.havok[1] + (g[1] - self.global[1]) as f32,
            self.havok[2] + (g[2] - self.global[2]) as f32,
        ]
    }

    pub fn global_for_havok(&self, h: [f32; 3]) -> [f64; 3] {
        [
            self.global[0] + (h[0] - self.havok[0]) as f64,
            self.global[1] + (h[1] - self.havok[1]) as f64,
            self.global[2] + (h[2] - self.havok[2]) as f64,
        ]
    }

    pub fn havok_for_mc(&self, m: [f64; 3]) -> [f32; 3] {
        self.havok_for_global(global_from_mc(m))
    }

    pub fn mc_for_havok(&self, h: [f32; 3]) -> [f64; 3] {
        mc_from_global(self.global_for_havok(h))
    }
}

/// Minecraft yaw/pitch (degrees) looking along an Elden Ring direction.
/// MC: yaw 0 looks +Z, yaw 90 looks -X; pitch +90 looks down.
pub fn mc_look(forward_er: [f32; 3]) -> (f32, f32) {
    let d = mc_dir(forward_er);
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
    let (x, y, z) = (d[0] / len, d[1] / len, d[2] / len);
    let yaw = (-x).atan2(z).to_degrees();
    let pitch = -(y.clamp(-1.0, 1.0)).asin().to_degrees();
    (yaw, pitch)
}
