//! Keyboard and mouse -> Minecraft's input ring (SDL scancodes), polled once per frame while the
//! Elden Ring window has focus. First version: polling, no swallowing (Elden Ring's own actions
//! are switched off on the player instead, see movement.rs).

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::link::Link;
use crate::proto::*;

/// (Windows virtual key, SDL scancode). Esc stays with Elden Ring (its menu).
const KEYS: &[(i32, u16)] = &[
    (0x41, 4), (0x42, 5), (0x43, 6), (0x44, 7), (0x45, 8), (0x46, 9), (0x47, 10), (0x48, 11),
    (0x49, 12), (0x4A, 13), (0x4B, 14), (0x4C, 15), (0x4D, 16), (0x4E, 17), (0x4F, 18), (0x50, 19),
    (0x51, 20), (0x52, 21), (0x53, 22), (0x54, 23), (0x55, 24), (0x56, 25), (0x57, 26), (0x58, 27),
    (0x59, 28), (0x5A, 29),
    (0x31, 30), (0x32, 31), (0x33, 32), (0x34, 33), (0x35, 34), (0x36, 35), (0x37, 36), (0x38, 37),
    (0x39, 38), (0x30, 39),
    (0x0D, 40), // Enter
    (0x08, 42), // Backspace
    (0x09, 43), // Tab
    (0x20, 44), // Space
    (0xBD, 45), // -
    (0xBB, 46), // =
    (0xBA, 51), // ;
    (0xBF, 56), // /
    (0xBE, 55), // .
    (0xBC, 54), // ,
    (0x70, 58), // F1
    (0x72, 60), // F3
    (0x74, 62), // F5
    (0x26, 82), (0x28, 81), (0x25, 80), (0x27, 79), // arrows
    (0xA2, 224), // LCtrl
    (0xA0, 225), // LShift
    (0xA4, 226), // LAlt
];

/// (Windows virtual key, SDL mouse button).
const BUTTONS: &[(i32, u16)] = &[(0x01, 1), (0x04, 2), (0x02, 3)];

pub struct Input {
    keys_down: Vec<bool>,
    buttons_down: Vec<bool>,
    focused: bool,
}

fn down(vk: i32) -> bool {
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

pub fn game_has_focus() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, &mut pid);
        pid == GetCurrentProcessId()
    }
}

/// The character a key types (US layout), for Minecraft's chat and text boxes.
fn typed_char(vk: i32, shift: bool) -> Option<char> {
    let c = match vk {
        0x41..=0x5A => {
            let c = (b'a' + (vk - 0x41) as u8) as char;
            if shift { c.to_ascii_uppercase() } else { c }
        }
        0x30..=0x39 => {
            let d = (vk - 0x30) as usize;
            if shift { [')', '!', '@', '#', '$', '%', '^', '&', '*', '('][d] } else { (b'0' + d as u8) as char }
        }
        0x20 => ' ',
        0xBD => if shift { '_' } else { '-' },
        0xBB => if shift { '+' } else { '=' },
        0xBA => if shift { ':' } else { ';' },
        0xBF => if shift { '?' } else { '/' },
        0xBE => if shift { '>' } else { '.' },
        0xBC => if shift { '<' } else { ',' },
        _ => return None,
    };
    Some(c)
}

impl Input {
    pub fn new() -> Self {
        Input { keys_down: vec![false; KEYS.len()], buttons_down: vec![false; BUTTONS.len()], focused: false }
    }

    /// `text_mode`: a Minecraft screen (chat, inventory) is open, so typed characters go as text too.
    pub fn poll(&mut self, link: &Link, enabled: bool, text_mode: bool) {
        let focused = enabled && game_has_focus();
        if !focused {
            if self.focused {
                link.push_input(IN_RELEASE_ALL, 0, 0, 0, 0);
                self.keys_down.iter_mut().for_each(|d| *d = false);
                self.buttons_down.iter_mut().for_each(|d| *d = false);
            }
            self.focused = false;
            return;
        }
        self.focused = true;
        let shift = down(0x10);
        for (i, &(vk, sdl)) in KEYS.iter().enumerate() {
            let now = down(vk);
            if now != self.keys_down[i] {
                self.keys_down[i] = now;
                link.push_input(IN_KEY, sdl, now as i32, 0, 0);
                if now && text_mode {
                    if let Some(c) = typed_char(vk, shift) {
                        link.push_input(5, 0, c as i32, 0, 0);
                    }
                }
            }
        }
        for (i, &(vk, sdl)) in BUTTONS.iter().enumerate() {
            let now = down(vk);
            if now != self.buttons_down[i] {
                self.buttons_down[i] = now;
                link.push_input(IN_MOUSE_BUTTON, sdl, now as i32, 0, 0);
            }
        }
    }
}
