//! The shared-memory mapping (Elden Ring creates it, Minecraft opens it). Port of SkyCraft's Link.cpp.

use std::ptr::null;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, PAGE_READWRITE};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::log;
use crate::proto::*;

const MC_TIMEOUT_MS: u64 = 3000;

pub struct Link {
    base: *mut u8,
    overlay_front: AtomicU32,
}

unsafe impl Send for Link {}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Link {
    pub fn create() -> Option<Link> {
        let name = wide(MAPPING_NAME);
        let size = MAPPING_BYTES as u64;
        let mapping = unsafe {
            CreateFileMappingW(INVALID_HANDLE_VALUE, null(), PAGE_READWRITE, (size >> 32) as u32, size as u32, name.as_ptr())
        };
        let err = unsafe { GetLastError() };
        if mapping.is_null() {
            log!("CreateFileMapping failed ({err})");
            return None;
        }
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_ALL_ACCESS, 0, 0, 0) };
        if view.Value.is_null() {
            log!("MapViewOfFile failed ({})", unsafe { GetLastError() });
            return None;
        }
        let base = view.Value as *mut u8;
        let link = Link { base, overlay_front: AtomicU32::new(2) };
        unsafe {
            // Reset everything the host owns; keep Minecraft's half if it is already attached.
            std::ptr::write_bytes(base.add(OFF_SKY_STATE), 0, size_of::<SkyState>());
            std::ptr::write_bytes(base.add(OFF_OVERLAY_CTL), 0, 0x100);
            std::ptr::write_bytes(base.add(OFF_INPUT_RING), 0, INPUT_RING_DATA_OFF);
            std::ptr::write_bytes(base.add(OFF_COLLISION_RING), 0, COL_RING_DATA_OFF);
            std::ptr::write_bytes(base.add(OFF_ACTOR_TABLE), 0, 0x40);
            std::ptr::write_bytes(base.add(OFF_EVENT_RING), 0, 0x80);
            std::ptr::write_bytes(base.add(OFF_WORLD_ENTITIES), 0, 0x40);
            std::ptr::write_bytes(base.add(OFF_RENDER_RING), 0, REN_RING_DATA_OFF);
            let h = base.add(OFF_HEADER) as *mut Header;
            (*h).version = VERSION;
            (*h).host_pid = GetCurrentProcessId();
            (*h).host_heartbeat_ms = GetTickCount64();
            link.a32(OFF_HEADER).store(MAGIC, Ordering::Release);
        }
        log!(
            "shared memory {} ({} MB, {})",
            MAPPING_NAME,
            MAPPING_BYTES >> 20,
            if err == ERROR_ALREADY_EXISTS { "reused" } else { "created" }
        );
        Some(link)
    }

    fn a32(&self, off: usize) -> &AtomicU32 {
        unsafe { &*(self.base.add(off) as *const AtomicU32) }
    }

    fn a64(&self, off: usize) -> &AtomicU64 {
        unsafe { &*(self.base.add(off) as *const AtomicU64) }
    }

    /// Takes Minecraft's newest overlay frame (HUD, hotbar, open screens) if there is one.
    pub fn acquire_overlay(&self) -> bool {
        let state = self.a32(OFF_OVERLAY_CTL);
        if state.load(Ordering::Acquire) & 4 == 0 {
            return false;
        }
        let old = state.swap(self.overlay_front.load(Ordering::Relaxed), Ordering::AcqRel);
        self.overlay_front.store(old & 3, Ordering::Relaxed);
        true
    }

    /// The overlay frame we hold: (width, height, bottom-up, RGBA pixels).
    pub fn overlay(&self) -> (u32, u32, bool, &[u8]) {
        let i = self.overlay_front.load(Ordering::Relaxed) as usize;
        unsafe {
            let hdr = self.base.add(OFF_OVERLAY_CTL + 0x40 + i * 0x40);
            let w = (hdr as *const u32).read_volatile().min(MAX_OVERLAY_W as u32);
            let h = (hdr.add(4) as *const u32).read_volatile().min(MAX_OVERLAY_H as u32);
            let flags = (hdr.add(8) as *const u32).read_volatile();
            let px = std::slice::from_raw_parts(self.base.add(OFF_OVERLAY_PIXELS + i * OVERLAY_SLOT_BYTES), (w * h * 4) as usize);
            (w, h, flags & 1 != 0, px)
        }
    }

    pub fn heartbeat(&self) {
        self.a64(OFF_HEADER + 0x10).store(unsafe { GetTickCount64() }, Ordering::Release);
    }

    pub fn mc_alive(&self) -> bool {
        let last = self.a64(OFF_HEADER + 0x18).load(Ordering::Acquire);
        last != 0 && unsafe { GetTickCount64() }.saturating_sub(last) < MC_TIMEOUT_MS
    }

    pub fn mc_pid(&self) -> u32 {
        self.a32(OFF_HEADER + 0x0C).load(Ordering::Acquire)
    }

    pub fn write_sky_state(&self, st: &SkyState) {
        let seq = self.a32(OFF_SKY_STATE);
        let s = seq.load(Ordering::Relaxed);
        seq.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        unsafe {
            std::ptr::copy_nonoverlapping(
                (st as *const SkyState as *const u8).add(4),
                self.base.add(OFF_SKY_STATE + 4),
                size_of::<SkyState>() - 4,
            );
        }
        seq.store(s.wrapping_add(2), Ordering::Release);
    }

    fn read_seqlock<T: Copy + Default>(&self, off: usize, bytes: usize) -> Option<T> {
        let seq = self.a32(off);
        for _ in 0..64 {
            let s1 = seq.load(Ordering::Acquire);
            if s1 & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let mut out = T::default();
            unsafe {
                std::ptr::copy_nonoverlapping(self.base.add(off), &mut out as *mut T as *mut u8, bytes.min(size_of::<T>()));
            }
            fence(Ordering::Acquire);
            if seq.load(Ordering::Relaxed) == s1 {
                return Some(out);
            }
        }
        None
    }

    pub fn read_mc_state(&self) -> Option<McState> {
        self.read_seqlock::<McState>(OFF_MC_STATE, size_of::<McState>())
    }

    pub fn read_world_entities_head(&self) -> Option<WorldEntitiesHead> {
        self.read_seqlock::<WorldEntitiesHead>(OFF_WORLD_ENTITIES, size_of::<WorldEntitiesHead>())
    }

    pub fn push_input(&self, kind: u16, code: u16, a: i32, b: i32, c: i32) {
        let ring = OFF_INPUT_RING;
        let head = self.a64(ring + INPUT_RING_HEAD_OFF).load(Ordering::Relaxed);
        let tail = self.a64(ring + INPUT_RING_TAIL_OFF).load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= INPUT_RING_ENTRIES {
            return;
        }
        let idx = (head & (INPUT_RING_ENTRIES - 1)) as usize;
        unsafe {
            let entry = self.base.add(ring + INPUT_RING_DATA_OFF + idx * size_of::<InputEvent>()) as *mut InputEvent;
            entry.write_volatile(InputEvent { kind, code, a, b, c });
        }
        self.a64(ring + INPUT_RING_HEAD_OFF).store(head + 1, Ordering::Release);
    }

    /// One message on the collision ring; false when the ring is full (try again next frame).
    pub fn write_collision(&self, kind: u32, payload: &[u8]) -> bool {
        let ring = OFF_COLLISION_RING;
        let size = COL_RING_DATA_BYTES as u64;
        let msg_bytes = ((size_of::<ColMsgHeader>() + payload.len() + 7) & !7) as u64;
        if msg_bytes > size / 2 {
            log!("collision message too large ({msg_bytes} bytes)");
            return false;
        }
        let mut head = self.a64(ring + COL_RING_HEAD_OFF).load(Ordering::Relaxed);
        let tail = self.a64(ring + COL_RING_TAIL_OFF).load(Ordering::Acquire);
        let mut pos = head % size;
        let pad = if pos + msg_bytes > size { size - pos } else { 0 };
        if size - (head - tail) < msg_bytes + pad {
            return false;
        }
        let data = unsafe { self.base.add(ring + COL_RING_DATA_OFF) };
        unsafe {
            if pad != 0 {
                (data.add(pos as usize) as *mut ColMsgHeader).write_unaligned(ColMsgHeader { kind: COL_PAD, payload_bytes: 0 });
                head += pad;
                pos = 0;
            }
            (data.add(pos as usize) as *mut ColMsgHeader)
                .write_unaligned(ColMsgHeader { kind, payload_bytes: payload.len() as u32 });
            std::ptr::copy_nonoverlapping(payload.as_ptr(), data.add(pos as usize + size_of::<ColMsgHeader>()), payload.len());
        }
        self.a64(ring + COL_RING_HEAD_OFF).store(head + msg_bytes, Ordering::Release);
        true
    }

    /// Hands every pending render-ring message to `f(type, payload)`, up to `max_bytes`.
    pub fn drain_render(&self, max_bytes: u64, mut f: impl FnMut(u32, &[u8])) {
        let ring = OFF_RENDER_RING;
        let size = REN_RING_DATA_BYTES as u64;
        let head = self.a64(ring + REN_RING_HEAD_OFF).load(Ordering::Acquire);
        let mut tail = self.a64(ring + REN_RING_TAIL_OFF).load(Ordering::Relaxed);
        let data = unsafe { self.base.add(ring + REN_RING_DATA_OFF) };
        let mut done = 0u64;
        while tail < head && done < max_bytes {
            let pos = tail % size;
            let hdr = unsafe { (data.add(pos as usize) as *const ColMsgHeader).read_unaligned() };
            if hdr.kind == REN_PAD {
                tail += size - pos;
                continue;
            }
            let payload = unsafe {
                std::slice::from_raw_parts(data.add(pos as usize + size_of::<ColMsgHeader>()), hdr.payload_bytes as usize)
            };
            f(hdr.kind, payload);
            let msg_bytes = ((size_of::<ColMsgHeader>() as u64 + hdr.payload_bytes as u64) + 7) & !7;
            tail += msg_bytes;
            done += msg_bytes;
        }
        self.a64(ring + REN_RING_TAIL_OFF).store(tail, Ordering::Release);
    }
}
