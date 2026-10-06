//! Start-up helpers: wait for the game window before touching game memory; diagnostics.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{BOOL, CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, RECT};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindowVisible};

unsafe extern "system" fn find_ours(hwnd: HWND, found: LPARAM) -> BOOL {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
            *(found as *mut bool) = true;
            return 0;
        }
    }
    1
}

unsafe extern "system" fn find_hwnd(hwnd: HWND, out: LPARAM) -> BOOL {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
            *(out as *mut HWND) = hwnd;
            return 0;
        }
    }
    1
}

/// The game window's client size, for Minecraft's HUD resolution.
pub fn game_client_size() -> Option<(u32, u32)> {
    let mut hwnd: HWND = std::ptr::null_mut();
    unsafe {
        EnumWindows(Some(find_hwnd), &mut hwnd as *mut HWND as LPARAM);
        if hwnd.is_null() {
            return None;
        }
        let mut r: RECT = std::mem::zeroed();
        if GetClientRect(hwnd, &mut r) == 0 {
            return None;
        }
        let (w, h) = ((r.right - r.left) as u32, (r.bottom - r.top) as u32);
        (w > 0 && h > 0).then_some((w, h))
    }
}

pub fn game_window_up() -> bool {
    let mut found = false;
    unsafe {
        EnumWindows(Some(find_ours), &mut found as *mut bool as LPARAM);
    }
    found
}

pub fn wait_for_game_window(timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if game_window_up() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

pub fn process_running(name: &str) -> bool {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(name) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}
