//! Shared-memory layout: SkyCraft protocol v11 (MIT, chasmlol), byte for byte.
//! Source of truth: protocol/skycraft_protocol.h. The Minecraft side (SkyCraft's Fabric mod,
//! unchanged) is pointed at our mapping name with -Dskycraft.link=EldenCraft_v1.

#![allow(dead_code)]

pub const MAGIC: u32 = 0x4359_4B53; // "SKYC"
pub const VERSION: u32 = 11;
/// Unprefixed: created in this login session's namespace, the same as "Local\" (and no backslash
/// to escape in Prism's instance.cfg JVM arguments).
pub const MAPPING_NAME: &str = "EldenCraft_v1";

pub const OFF_HEADER: usize = 0x0;
pub const OFF_SKY_STATE: usize = 0x100;
pub const OFF_MC_STATE: usize = 0x200;
pub const OFF_OVERLAY_CTL: usize = 0x300;
pub const OFF_WATER_GRID: usize = 0x400;
pub const OFF_INPUT_RING: usize = 0x1000;
pub const OFF_ACTOR_TABLE: usize = 0x12000;
pub const OFF_EVENT_RING: usize = 0x17000;
pub const OFF_WORLD_ENTITIES: usize = 0x1C000;
pub const OFF_COLLISION_RING: usize = 0x20000;
pub const COLLISION_RING_BYTES: usize = 32 << 20;
pub const OFF_OVERLAY_PIXELS: usize = OFF_COLLISION_RING + COLLISION_RING_BYTES;
pub const MAX_OVERLAY_W: usize = 3840;
pub const MAX_OVERLAY_H: usize = 2160;
pub const OVERLAY_SLOT_BYTES: usize = MAX_OVERLAY_W * MAX_OVERLAY_H * 4;
pub const OVERLAY_SLOTS: usize = 3;
pub const OFF_RENDER_RING: usize = OFF_OVERLAY_PIXELS + OVERLAY_SLOT_BYTES * OVERLAY_SLOTS;
pub const RENDER_RING_BYTES: usize = 64 << 20;
pub const MAPPING_BYTES: usize = OFF_RENDER_RING + RENDER_RING_BYTES;

#[repr(C)]
pub struct Header {
    pub magic: u32,
    pub version: u32,
    pub host_pid: u32,
    pub mc_pid: u32,
    pub host_heartbeat_ms: u64,
    pub mc_heartbeat_ms: u64,
}
const _: () = assert!(size_of::<Header>() == 0x20);

pub const SKY_IN_GAME: u32 = 1 << 0;
pub const SKY_MENU_OPEN: u32 = 1 << 1;
pub const SKY_LOADING: u32 = 1 << 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkyState {
    pub seq: u32,
    pub flags: u32,
    pub world_id: u32,
    pub collision_epoch: u32,
    pub pos_x: f64,
    pub pos_y: f64,
    pub pos_z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub teleport_seq: u32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub game_hour: f32,
}
const _: () = assert!(size_of::<SkyState>() == 0x40);

pub const MC_IN_WORLD: u32 = 1 << 0;
pub const MC_SCREEN_OPEN: u32 = 1 << 1;
pub const MC_ON_GROUND: u32 = 1 << 2;
pub const MC_SNEAKING: u32 = 1 << 3;
pub const MC_SPRINTING: u32 = 1 << 4;
pub const MC_DEAD: u32 = 1 << 5;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct McState {
    pub seq: u32,
    pub flags: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub eye_height: f32,
    pub sensitivity: f32,
    pub teleport_ack: u32,
    pub gui_scale: u32,
    pub frame_counter: u64,
    pub fov_deg: f32,
    pub bob_phase: f32,
    pub bob_amount: f32,
    pub pad4c: u32,
    pub eye_x: f64,
    pub eye_y: f64,
    pub eye_z: f64,
    pub tick_qpc: i64,
    pub prev_x: f64,
    pub prev_y: f64,
    pub prev_z: f64,
    pub cur_x: f64,
    pub cur_y: f64,
    pub cur_z: f64,
    pub tick_eye_o: f32,
    pub tick_eye: f32,
    pub walk_dist_o: f32,
    pub walk_dist: f32,
    pub bob_o: f32,
    pub bob: f32,
    pub tick_ms: f32,
    pub tick_pad: u32,
    pub camera_mode: u32,
    pub camera_distance: f32,
}
const _: () = assert!(size_of::<McState>() == 0xC8);

// ---- input ring --------------------------------------------------------------------------
pub const INPUT_RING_ENTRIES: u64 = 4096;
pub const INPUT_RING_HEAD_OFF: usize = 0x00;
pub const INPUT_RING_TAIL_OFF: usize = 0x40;
pub const INPUT_RING_DATA_OFF: usize = 0x80;

pub const IN_KEY: u16 = 1;
pub const IN_MOUSE_BUTTON: u16 = 2;
pub const IN_SCROLL: u16 = 3;
pub const IN_RELEASE_ALL: u16 = 6;
pub const IN_HURT: u16 = 7;
pub const IN_OPEN_MENU: u16 = 8;
/// EldenCraft's Fabric patch: set Minecraft's health (a = hearts x 100).
pub const IN_SET_HEALTH: u16 = 20;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputEvent {
    pub kind: u16,
    pub code: u16,
    pub a: i32,
    pub b: i32,
    pub c: i32,
}
const _: () = assert!(size_of::<InputEvent>() == 16);

// ---- actor table (host -> MC, seqlock) and event ring (MC -> host) -------------------------
pub const MAX_ACTORS: usize = 256;
pub const AT_COUNT_OFF: usize = 0x04;
pub const AT_RECORDS_OFF: usize = 0x40;
pub const ACTOR_RECORD_BYTES: usize = 64;
pub const ACTOR_HOSTILE: u32 = 1;
pub const ACTOR_DEAD: u32 = 1 << 1;
pub const ACTOR_IN_COMBAT: u32 = 1 << 3;

pub const EVENT_RING_ENTRIES: u64 = 512;
pub const EV_HEAD_OFF: usize = 0x00;
pub const EV_TAIL_OFF: usize = 0x40;
pub const EV_DATA_OFF: usize = 0x80;
pub const EV_HIT_ACTOR: i32 = 1;
pub const EV_PLAYER_DIED: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ActorRecord {
    pub form_id: i32,
    pub flags: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub width: f32,
    pub height: f32,
    pub health_frac: f32,
    pub level: u16,
    pub pad: u16,
    pub name: [u8; 24],
}
const _: () = assert!(size_of::<ActorRecord>() == ACTOR_RECORD_BYTES);

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Event {
    pub kind: i32,
    pub form_id: i32,
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub flags: i32,
    pub weapon: i32,
}
const _: () = assert!(size_of::<Event>() == 32);

// ---- collision ring ----------------------------------------------------------------------
pub const COL_RING_HEAD_OFF: usize = 0x00;
pub const COL_RING_TAIL_OFF: usize = 0x40;
pub const COL_RING_DATA_OFF: usize = 0x80;
pub const COL_RING_DATA_BYTES: usize = COLLISION_RING_BYTES - COL_RING_DATA_OFF;

pub const COL_PAD: u32 = 0;
pub const COL_CLEAR: u32 = 1;
pub const COL_REGION: u32 = 2;
pub const COL_TRIS: u32 = 3;

pub const TRI_TERRAIN: u32 = 1 << 3;

/// Minecraft's collision regions are cubes of this many blocks (SkyCollision.REGION_SIZE).
pub const REGION_SIZE: i32 = 8;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColMsgHeader {
    pub kind: u32,
    pub payload_bytes: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColRegion {
    pub min_x: i32,
    pub min_y: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub max_z: i32,
    pub epoch: u32,
    pub count: u32,
}
const _: () = assert!(size_of::<ColRegion>() == 32);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColTri {
    pub v: [f32; 9],
    pub flags: u32,
}
const _: () = assert!(size_of::<ColTri>() == 40);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub pad: u32,
    pub bits: [u64; 8],
}
const _: () = assert!(size_of::<ColBlock>() == 80);

// ---- world entities (MC -> host, seqlock) ------------------------------------------------
pub const MAX_WORLD_ENTITIES: usize = 160;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WorldEntity {
    pub kind: u32,
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
    pub ext: [f32; 3],
    pub uv: [[f32; 4]; 3],
    pub tint: u32,
}
const _: () = assert!(size_of::<WorldEntity>() == 96);

/// Only the head of WorldEntities (selection outline); entities follow at +0x40.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct WorldEntitiesHead {
    pub seq: u32,
    pub count: u32,
    pub has_selection: u32,
    pub sel_min: [f32; 3],
    pub sel_max: [f32; 3],
}

// ---- render ring (MC -> host) ------------------------------------------------------------
pub const REN_RING_HEAD_OFF: usize = 0x00;
pub const REN_RING_TAIL_OFF: usize = 0x40;
pub const REN_RING_DATA_OFF: usize = 0x80;
pub const REN_RING_DATA_BYTES: usize = RENDER_RING_BYTES - REN_RING_DATA_OFF;

pub const REN_PAD: u32 = 0;
pub const REN_ATLAS: u32 = 1;
pub const REN_SECTION: u32 = 2;
pub const REN_CLEAR_ALL: u32 = 3;
pub const REN_ATLAS_REGION: u32 = 7;
pub const REN_TEXTURE: u32 = 4;
pub const REN_SOLIDS: u32 = 10;
pub const REN_AVATAR: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenTexture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenAvatar {
    pub batch_count: u32,
    pub vertex_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenBatch {
    pub texture: u32,
    pub first: u32,
    pub count: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenSection {
    pub sx: i32,
    pub sy: i32,
    pub sz: i32,
    pub vertex_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub color: u32,
    pub light: u32,
    pub flags: u32,
}
const _: () = assert!(size_of::<RenVertex>() == 32);
