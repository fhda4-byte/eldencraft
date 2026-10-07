//! First-person camera (sheet: systems.camera). Elden Ring's camera keeps its rotation (mouse / right
//! stick) but is moved to Steve's eyes, with Minecraft's field of view (sprint widening included) and
//! Minecraft's view bobbing. Written right after the game computes its camera (ChrIns_PostPhysics)
//! and again just before drawing (Draw_Pre), so nothing in between can move it back.

use std::sync::Mutex;

use eldenring::cs::CSCamera;
use fromsoftware_shared::FromStatic;

#[derive(Clone, Copy)]
pub struct Override {
    /// Eye position, Havok space.
    pub pos: [f32; 3],
    /// Vertical field of view, radians.
    pub fov: f32,
}

static CURRENT: Mutex<Option<Override>> = Mutex::new(None);
/// Elden Ring's own near plane, put back when first person ends.
static ORIGINAL_NEAR: Mutex<Option<f32>> = Mutex::new(None);

pub fn set(o: Option<Override>) {
    let mut c = match CURRENT.lock() {
        Ok(c) => c,
        Err(p) => p.into_inner(),
    };
    *c = o;
}

pub fn apply() {
    let o = match CURRENT.lock() {
        Ok(c) => *c,
        Err(p) => *p.into_inner(),
    };
    let mut near = match ORIGINAL_NEAR.lock() {
        Ok(n) => n,
        Err(p) => p.into_inner(),
    };
    let Ok(c) = (unsafe { CSCamera::instance_mut() }) else { return };
    let cam = &mut c.pers_cam_1;
    let Some(o) = o else {
        if let Some(n) = near.take() {
            cam.near_plane = n;
        }
        return;
    };
    {
        cam.matrix.3.0 = o.pos[0];
        cam.matrix.3.1 = o.pos[1];
        cam.matrix.3.2 = o.pos[2];
        cam.fov = o.fov;
        if cam.near_plane > 0.05 {
            near.get_or_insert(cam.near_plane);
            cam.near_plane = 0.05;
        }
    }
}

/// Minecraft's view bobbing offset (camera right, camera up) in blocks, from its walk distance and
/// bob amount interpolated to now.
pub fn bob(walk: f32, amount: f32) -> (f32, f32) {
    let p = walk * std::f32::consts::PI;
    (p.sin() * amount * 0.5, -(p.cos() * amount).abs())
}

/// Where between Minecraft's last two ticks we are now (0..1), for smooth bobbing.
pub fn partial_tick(tick_qpc: i64, tick_ms: f32) -> f32 {
    use windows_sys::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut now, mut freq) = (0i64, 0i64);
    unsafe {
        QueryPerformanceCounter(&mut now);
        QueryPerformanceFrequency(&mut freq);
    }
    if freq <= 0 || tick_ms <= 0.0 || tick_qpc == 0 {
        return 1.0;
    }
    let ms = (now - tick_qpc) as f64 * 1000.0 / freq as f64;
    (ms / tick_ms as f64).clamp(0.0, 1.0) as f32
}
