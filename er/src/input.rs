//! Keyboard and mouse -> Minecraft's input ring (SDL scancodes), polled once per frame while the
//! Elden Ring window has focus. First version: polling, no swallowing (Elden Ring's own actions
//! are switched off on the player instead, see movement.rs).

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eldenring::cs::UserInputKey;
use eldenring::fd4::FD4PadManager;
use fromsoftware_shared::FromStatic;

use crate::link::Link;
use crate::proto::*;

/// (Windows virtual key, SDL scancode). Esc stays with Elden Ring (its menu).
const KEYS: &[(i32, u16)] = &[
    (0x41, 4), (0x42, 5), (0x43, 6), (0x44, 7), (0x46, 9), (0x47, 10), (0x48, 11),
    (0x49, 12), (0x4A, 13), (0x4B, 14), (0x4C, 15), (0x4D, 16), (0x4E, 17), (0x4F, 18), (0x50, 19),
    (0x52, 21), (0x53, 22), (0x54, 23), (0x55, 24), (0x56, 25), (0x57, 26), (0x58, 27),
    (0x59, 28), (0x5A, 29),
    (0x31, 30), (0x32, 31), (0x33, 32), (0x34, 33), (0x35, 34), (0x36, 35), (0x37, 36), (0x38, 37),
    (0x39, 38), (0x30, 39),
    (0x0D, 40), // Enter
    (0x08, 42), // Backspace
    (0x09, 8),  // Tab -> Minecraft's E (inventory); E itself stays Elden Ring's interact
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
/// Right mouse stays Elden Ring's guard; Minecraft's place / use is on R (Elden Ring's use-item key,
/// switched off while Minecraft drives).
const BUTTONS: &[(i32, u16)] = &[(0x01, 1), (0x04, 2), (0x52, 3)];

/// Elden Ring's own controls (any controller it supports: PS5 DualSense, Xbox, ...) -> Minecraft.
/// Read through the game's input layer, so the player's Elden Ring button layout applies.
/// (Elden Ring action, Minecraft input kind, code)
const PAD_HOLD: &[(UserInputKey, u16, u16)] = &[
    (UserInputKey::MoveForwards, IN_KEY, 26),  // W
    (UserInputKey::MoveBackwards, IN_KEY, 22), // S
    (UserInputKey::MoveLeft, IN_KEY, 4),       // A
    (UserInputKey::MoveRight, IN_KEY, 7),      // D
    (UserInputKey::Jump, IN_KEY, 44),          // Space: jump
    (UserInputKey::Backstep, IN_KEY, 224),     // LCtrl: sprint (Elden Ring's dash button)
    (UserInputKey::Crouch, IN_KEY, 225),       // LShift: sneak
    (UserInputKey::Attack, IN_MOUSE_BUTTON, 1), // break / attack
    (UserInputKey::UseItem, IN_MOUSE_BUTTON, 3),
    (UserInputKey::SwitchItem, IN_KEY, 8),     // d-pad down: Minecraft inventory (R3 stays Elden Ring's lock-on)
    (UserInputKey::SwitchSpell, IN_KEY, 62),   // d-pad up: F5, first / third person
];

/// Keyboard/mouse in use this recently means controller mappings stay off (no double meanings).
const KEYBOARD_GRACE: Duration = Duration::from_millis(1500);

pub struct Input {
    keys_down: Vec<bool>,
    buttons_down: Vec<bool>,
    focused: bool,
    /// What Minecraft currently has held, per (kind, code), from keyboard and controller together.
    sent: HashMap<(u16, u16), bool>,
    keyboard_used: Option<Instant>,
    dpad_right: bool,
    dpad_left: bool,
    pub pad_active: bool,
    /// Everything held this frame (keyboard and controller), for the movement controller.
    held: HashMap<(u16, u16), bool>,
}

fn pad_state() -> Option<Vec<bool>> {
    let man = unsafe { FD4PadManager::instance() }.ok()?;
    let pad = man.get_in_game_pad()?;
    let mut v: Vec<bool> = PAD_HOLD.iter().map(|(k, _, _)| pad.poll_digital_input(*k)).collect();
    v.push(pad.poll_digital_input(UserInputKey::SwitchRightHandArmament));
    v.push(pad.poll_digital_input(UserInputKey::SwitchleftHandArmament));
    Some(v)
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
        Input {
            keys_down: vec![false; KEYS.len()],
            buttons_down: vec![false; BUTTONS.len()],
            focused: false,
            sent: HashMap::new(),
            keyboard_used: None,
            dpad_right: false,
            dpad_left: false,
            pad_active: false,
            held: HashMap::new(),
        }
    }

    fn set(&mut self, link: &Link, kind: u16, code: u16, down: bool) {
        let was = self.sent.get(&(kind, code)).copied().unwrap_or(false);
        if was != down {
            self.sent.insert((kind, code), down);
            link.push_input(kind, code, down as i32, 0, 0);
        }
    }

    fn is_held(&self, kind: u16, code: u16) -> bool {
        self.held.get(&(kind, code)).copied().unwrap_or(false)
    }

    /// Movement keys held this frame: W/S/A/D, Space, Ctrl, Shift (or the controller's equivalents).
    pub fn movement(&self) -> crate::walk::MoveInput {
        let k = |c| self.is_held(IN_KEY, c) as i32 as f64;
        crate::walk::MoveInput {
            forward: k(26) - k(22),
            strafe: k(7) - k(4),
            jump: self.is_held(IN_KEY, 44),
            sprint: self.is_held(IN_KEY, 224),
            sneak: self.is_held(IN_KEY, 225),
        }
    }

    /// `text_mode`: a Minecraft screen (chat, inventory) is open, so typed characters go as text too.
    pub fn poll(&mut self, link: &Link, enabled: bool, text_mode: bool) {
        let focused = enabled && game_has_focus();
        if !focused {
            if self.focused {
                link.push_input(IN_RELEASE_ALL, 0, 0, 0, 0);
                self.keys_down.iter_mut().for_each(|d| *d = false);
                self.buttons_down.iter_mut().for_each(|d| *d = false);
                self.sent.clear();
            }
            self.focused = false;
            self.held.clear();
            return;
        }
        self.focused = true;
        let shift = down(0x10);
        let mut wanted: HashMap<(u16, u16), bool> = HashMap::new();
        let mut any_keyboard = false;
        for (i, &(vk, sdl)) in KEYS.iter().enumerate() {
            let now = down(vk);
            any_keyboard |= now;
            if now && !self.keys_down[i] && text_mode {
                if let Some(c) = typed_char(vk, shift) {
                    link.push_input(5, 0, c as i32, 0, 0);
                }
            }
            self.keys_down[i] = now;
            *wanted.entry((IN_KEY, sdl)).or_insert(false) |= now;
        }
        for (i, &(vk, sdl)) in BUTTONS.iter().enumerate() {
            let now = down(vk);
            any_keyboard |= now;
            self.buttons_down[i] = now;
            *wanted.entry((IN_MOUSE_BUTTON, sdl)).or_insert(false) |= now;
        }
        if any_keyboard {
            self.keyboard_used = Some(Instant::now());
        }
        let keyboard_recent = self.keyboard_used.map_or(false, |t| t.elapsed() < KEYBOARD_GRACE);
        self.pad_active = false;
        if !keyboard_recent && !text_mode {
            if let Some(state) = pad_state() {
                for (k, &(_, kind, code)) in PAD_HOLD.iter().enumerate() {
                    if state[k] {
                        self.pad_active = true;
                    }
                    *wanted.entry((kind, code)).or_insert(false) |= state[k];
                }
                // D-pad right / left: next / previous hotbar slot (one step per press).
                let (right, left) = (state[PAD_HOLD.len()], state[PAD_HOLD.len() + 1]);
                if right && !self.dpad_right {
                    link.push_input(IN_SCROLL, 0, -120, 0, 0);
                }
                if left && !self.dpad_left {
                    link.push_input(IN_SCROLL, 0, 120, 0, 0);
                }
                self.dpad_right = right;
                self.dpad_left = left;
            }
        }
        for (&(kind, code), &down) in &wanted {
            self.set(link, kind, code, down);
        }
        self.held = if text_mode { HashMap::new() } else { wanted };
    }
}
