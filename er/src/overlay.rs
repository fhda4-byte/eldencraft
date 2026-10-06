//! The screen overlay: draws Minecraft's HUD over the finished Elden Ring frame, in screen pixels
//! (sheet: systems.hud_overlay). Dear ImGui through hudhook (MIT, veeenu) hooks Elden Ring's DirectX 12
//! present; we draw only flat rectangles on ImGui's background layer, no windows, no input capture.

use std::sync::Mutex;

use hudhook::hooks::dx12::ImguiDx12Hooks;
use hudhook::{Hudhook, ImguiRenderLoop};
use hudhook::windows::Win32::Foundation::HINSTANCE;

use crate::log;

#[derive(Clone, Copy)]
pub struct Quad {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// 0xAABBGGRR
    pub color: u32,
}

struct Shared {
    visible: bool,
    w: f32,
    h: f32,
    quads: Vec<Quad>,
    version: u64,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared { visible: false, w: 1920.0, h: 1080.0, quads: Vec::new(), version: 0 });

/// Called from the game thread whenever a new HUD frame is ready (or visibility changes).
pub fn publish(visible: bool, w: f32, h: f32, quads: &[Quad], version: u64) {
    let mut s = match SHARED.lock() {
        Ok(s) => s,
        Err(p) => p.into_inner(),
    };
    s.visible = visible;
    if s.version != version {
        s.version = version;
        s.w = w;
        s.h = h;
        s.quads.clear();
        s.quads.extend_from_slice(quads);
    }
}

struct Overlay {
    quads: Vec<Quad>,
    version: u64,
    frames: u64,
}

impl ImguiRenderLoop for Overlay {
    fn initialize<'a>(&'a mut self, ctx: &mut hudhook::imgui::Context, _render: &'a mut dyn hudhook::RenderContext) {
        // No imgui.ini in the game folder; leave Elden Ring's mouse cursor alone.
        ctx.set_ini_filename(None::<std::path::PathBuf>);
        ctx.io_mut().config_flags |=
            hudhook::imgui::ConfigFlags::NO_MOUSE_CURSOR_CHANGE | hudhook::imgui::ConfigFlags::NO_MOUSE;
        log!("overlay: ImGui ready");
    }

    fn render(&mut self, ui: &mut hudhook::imgui::Ui) {
        self.frames += 1;
        if self.frames == 1 {
            log!("overlay: first frame drawn by the screen hook");
        }
        let (visible, w, h) = {
            let s = match SHARED.lock() {
                Ok(s) => s,
                Err(p) => p.into_inner(),
            };
            if s.version != self.version {
                self.version = s.version;
                self.quads.clear();
                self.quads.extend_from_slice(&s.quads);
            }
            (s.visible, s.w, s.h)
        };
        if !visible || self.quads.is_empty() || w <= 0.0 || h <= 0.0 {
            return;
        }
        let [dw, dh] = ui.io().display_size;
        let (sx, sy) = (dw / w, dh / h);
        let list = ui.get_background_draw_list();
        for q in &self.quads {
            list.add_rect([q.x0 * sx, q.y0 * sy], [q.x1 * sx, q.y1 * sy], q.color).filled(true).build();
        }
    }
}

/// Hook Elden Ring's DirectX 12 present (call once the game window is up).
pub fn install(hmodule: usize) {
    let result = Hudhook::builder()
        .with::<ImguiDx12Hooks>(Overlay { quads: Vec::new(), version: 0, frames: 0 })
        .with_hmodule(HINSTANCE(hmodule as *mut core::ffi::c_void))
        .build()
        .apply();
    match result {
        Ok(()) => log!("overlay: DirectX 12 hook installed"),
        Err(e) => log!("overlay: DirectX 12 hook failed ({e:?})"),
    }
}
