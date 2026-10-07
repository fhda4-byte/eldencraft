//! EldenCraft, Elden Ring side: loaded by ModEngine2 (external_dlls), offline with Easy Anti-Cheat
//! off. Talks to a hidden Minecraft (SkyCraft's Fabric mod) over shared memory. Movement runs here
//! with Minecraft's physics on Elden Ring's real collision (walk.rs); Minecraft follows it and runs
//! blocks, inventory, health and hunger; Elden Ring runs and draws the world.
//! Design and status: sheets/*.json. Every system here is a row in sheets/systems.json.

mod actors;
mod blocks;
mod camera;
mod boot;
mod collision;
mod coords;
mod hud;
mod input;
mod launcher;
mod link;
mod log;
mod overlay;
mod proto;
mod walk;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use eldenring::cs::{CSCamera, CSTaskGroupIndex, CSTaskImp, RendMan, WorldChrMan};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};

use coords::Frame;
use proto::*;

/// After a map loads, wait this long before Minecraft takes the player (the game settles it first).
const SETTLE: Duration = Duration::from_secs(3);
/// SkyState flags of ours (EldenCraft's Fabric patch): Minecraft's player follows our position every
/// tick instead of moving itself, and whether that position is standing on the ground.
const EC_FOLLOW: u32 = 1 << 8;
const EC_ON_GROUND: u32 = 1 << 9;
/// First-person eye height (metres): Steve's 1.62 reads low next to Elden Ring's people and doors.
const EYE_HEIGHT: f64 = 1.7;
/// Elden Ring actions switched off while Minecraft drives: attacks, items, roll, magic, gestures,
/// guard, kicks, two-handing, Torrent. Walking, jumping, dashing, ladders and interact stay on.
fn block_actions(a: &mut eldenring::cs::ChrActions, on: bool) {
    a.set_r1(on);
    a.set_r2(on);
    a.set_l1(on);
    a.set_l2(on);
    a.set_use_item(on);
    a.set_l3(on);
    a.set_rolling(on);
    a.set_magic_r(on);
    a.set_magic_l(on);
    a.set_gesture(on);
    a.set_guard(on);
    a.set_light_kick(on);
    a.set_heavy_kick(on);
    a.set_change_style_r(on);
    a.set_change_style_l(on);
    a.set_rideon(on);
    a.set_magic_r2(on);
    a.set_magic_l2(on);
}

struct State {
    link: Option<link::Link>,
    input: input::Input,
    collision: collision::Collision,
    blocks: blocks::Blocks,
    hud: hud::Hud,
    er_phase: bool,
    viewport: (u32, u32),
    viewport_checked: Option<Instant>,
    cam_flip: Option<bool>,
    actors: actors::Actors,
    f5_done: bool,
    first_person: bool,
    launcher: launcher::Launcher,
    teleport_seq: u32,
    world_id: u32,
    last_block: i32,
    last_global: Option<[f64; 3]>,
    written_global: Option<[f64; 3]>,
    driving: bool,
    saved_gravity: Option<f32>,
    settled_since: Option<Instant>,
    last_f5: Option<Instant>,
    hid_model: bool,
    was_loading: bool,
    last_report: Instant,
    frames: u64,
    walker: walk::Walker,
    last_tick: Option<Instant>,
    dt: f64,
}

impl State {
    fn new() -> Self {
        State {
            link: link::Link::create(),
            input: input::Input::new(),
            collision: collision::Collision::new(),
            blocks: blocks::Blocks::new(),
            hud: hud::Hud::new(),
            er_phase: false,
            viewport: (1920, 1080),
            viewport_checked: None,
            cam_flip: None,
            actors: actors::Actors::new(),
            f5_done: false,
            first_person: false,
            launcher: launcher::Launcher::new(),
            teleport_seq: 1,
            world_id: 0,
            last_block: i32::MIN,
            last_global: None,
            written_global: None,
            driving: false,
            saved_gravity: None,
            settled_since: None,
            last_f5: None,
            hid_model: false,
            was_loading: false,
            last_report: Instant::now(),
            frames: 0,
            walker: walk::Walker::new(),
            last_tick: None,
            dt: 0.0,
        }
    }

    fn frame(&mut self) {
        self.frames += 1;
        let now = Instant::now();
        self.dt = self.last_tick.map_or(0.0, |t| (now - t).as_secs_f64());
        self.last_tick = Some(now);
        let Some(link) = self.link.take() else { return };
        self.frame_linked(&link);
        self.link = Some(link);
    }

    fn frame_linked(&mut self, link: &link::Link) {
        link.heartbeat();
        let mc_alive = link.mc_alive();
        self.launcher.tick(mc_alive);
        let mc = link.read_mc_state().unwrap_or_default();

        let mut sky = SkyState::default();
        if self.viewport_checked.map_or(true, |t| t.elapsed() > Duration::from_secs(2)) {
            self.viewport_checked = Some(Instant::now());
            if let Some(v) = boot::game_client_size() {
                if v != self.viewport {
                    log!("game window {}x{}", v.0, v.1);
                }
                self.viewport = v;
            }
        }
        sky.viewport_w = self.viewport.0;
        sky.viewport_h = self.viewport.1;
        sky.game_hour = 12.0;
        sky.teleport_seq = self.teleport_seq;
        sky.collision_epoch = self.collision.epoch;

        // The player (absent on the title screen and while loading).
        let player_ptr = unsafe { WorldChrMan::instance_mut() }
            .ok()
            .and_then(|w| w.main_player.as_mut())
            .map(|p| &mut **p as *mut eldenring::cs::PlayerIns);
        let Some(player_ptr) = player_ptr else {
            self.was_loading = true;
            sky.flags = SKY_LOADING;
            sky.world_id = self.world_id;
            link.write_sky_state(&sky);
            self.input.poll(link, false, false);
            self.release(None);
            self.hud.publish(false);
            camera::set(None);
            self.report(link, &mc, None);
            return;
        };
        let player = unsafe { &mut *player_ptr };
        // While a map loads the player exists with block m255_255_255_255 and a placeholder position.
        if player.current_block_id.area() == 255 || player.current_block_id.0 == -1 {
            self.settled_since = None;
            self.was_loading = true;
            sky.flags = SKY_LOADING;
            sky.world_id = self.world_id;
            link.write_sky_state(&sky);
            self.input.poll(link, false, false);
            self.release(Some(&mut *player));
            self.hud.publish(false);
            camera::set(None);
            self.report(link, &mc, None);
            return;
        }
        let settled = *self.settled_since.get_or_insert_with(Instant::now);

        // Where the player is, in every space we need.
        let (havok, local, block) = {
            let phys = &player.chr_ins.modules.physics;
            let bp = player.block_position;
            ([phys.position.0, phys.position.1, phys.position.2], [bp.x, bp.y, bp.z], player.current_block_id)
        };
        let (world_id, global) = coords::world_and_global(block, local);
        let frame = Frame { havok, global, world_id };
        if block.0 != self.last_block {
            log!(
                "block {} local ({:.2}, {:.2}, {:.2}) global ({:.2}, {:.2}, {:.2}) havok ({:.2}, {:.2}, {:.2})",
                block, local[0], local[1], local[2], global[0], global[1], global[2], havok[0], havok[1], havok[2]
            );
            self.last_block = block.0;
        }

        // A different map (or first frame): Minecraft drops collision and moves its player here.
        if world_id != self.world_id {
            log!("world {:08x} -> {:08x}", self.world_id, world_id);
            self.world_id = world_id;
            self.teleport_seq += 1;
            self.collision.reset(link);
            self.written_global = None;
        } else if self.was_loading {
            // Back from a loading screen (grace travel, death, a door between areas): Minecraft
            // follows to wherever Elden Ring put the player. Nothing else moves Minecraft's player:
            // Elden Ring pushing the character out of a wall must not lift Minecraft onto it.
            log!("after loading: Minecraft follows to ({:.1}, {:.1}, {:.1})", global[0], global[1], global[2]);
            self.teleport_seq += 1;
            self.collision.reset(link);
        }
        self.was_loading = false;
        self.last_global = Some(global);

        // Look direction: Elden Ring's camera is authoritative (Minecraft follows it).
        // The camera's rows: right, up, forward, position (Havok space). Which way "forward" points
        // is checked against the player (a third-person camera always looks at them).
        // The sign is learned once from a third-person camera (in first person the camera sits at
        // the player, so the check can't be made then).
        let cam = unsafe { CSCamera::instance() }.ok().map(|c| {
            let m = &c.pers_cam_1.matrix;
            let pos = [m.3.0, m.3.1, m.3.2];
            let fwd = [m.2.0, m.2.1, m.2.2];
            let to_player = [havok[0] - pos[0], havok[1] + 1.2 - pos[1], havok[2] - pos[2]];
            let dist = (to_player[0].powi(2) + to_player[1].powi(2) + to_player[2].powi(2)).sqrt();
            let dot = fwd[0] * to_player[0] + fwd[1] * to_player[1] + fwd[2] * to_player[2];
            ([[m.0.0, m.0.1, m.0.2], [m.1.0, m.1.1, m.1.2], fwd, pos], dist, dot)
        });
        let cam = cam.map(|(mut m, dist, dot)| {
            if self.cam_flip.is_none() && dist > 1.5 {
                self.cam_flip = Some(dot < 0.0);
                log!("camera: forward is {}row2", if dot < 0.0 { "-" } else { "+" });
            }
            if self.cam_flip.unwrap_or(dot < 0.0) {
                m[2] = [-m[2][0], -m[2][1], -m[2][2]];
            }
            (m, 0.0f32, 0.0f32)
        });
        let (yaw, pitch) = cam.map(|(m, _, _)| coords::mc_look(m[2])).unwrap_or((0.0, 0.0));

        let feet = coords::mc_from_global(global);

        // Minecraft takes the player once it is in its world, has followed our last teleport, and the
        // map has settled.
        let mut mc_ready = mc_alive
            && (mc.flags & MC_IN_WORLD) != 0
            && mc.teleport_ack == self.teleport_seq
            && settled.elapsed() > SETTLE;
        // Ladders: Elden Ring climbs them itself (Minecraft's player follows its position meanwhile).
        let on_ladder = player.chr_ins.modules.ladder.state != eldenring::cs::LadderState::None;
        if on_ladder {
            if !self.er_phase {
                log!("ladder: Elden Ring climbs");
            }
            self.er_phase = true;
            mc_ready = false;
        } else if self.er_phase {
            log!("ladder: done");
            self.er_phase = false;
        }
        let screen_open = (mc.flags & MC_SCREEN_OPEN) != 0;
        self.input.poll(link, mc_ready, screen_open);

        if mc_ready {
            // Elden Ring's own physics moves the player (ground, walls, stairs, caves: never through
            // its floor); Minecraft's player follows. Placed Minecraft blocks are added on top.
            if self.saved_gravity.is_none() {
                self.saved_gravity = Some(player.chr_ins.modules.physics.gravity_multiplier);
                log!("driving: Elden Ring physics, Minecraft follows (gravity {})", player.chr_ins.modules.physics.gravity_multiplier);
            }
            let prev = if self.walker.active { self.walker.pos } else { feet };
            let (fixed, on_block) = walk::block_fix(&self.blocks, prev, feet);
            let phys = &mut player.chr_ins.modules.physics;
            if fixed != feet {
                let h = frame.havok_for_mc(fixed);
                phys.position.0 = h[0];
                phys.position.1 = h[1];
                phys.position.2 = h[2];
                phys.chr_proxy_pos_update_requested = true;
            }
            // Standing on a Minecraft block: no Elden Ring fall building up underneath.
            phys.gravity_multiplier = if on_block { 0.0 } else { self.saved_gravity.unwrap_or(1.0) };
            if on_block {
                player.chr_ins.modules.fall.fall_timer = 0.0;
            }
            self.walker.pos = fixed;
            self.walker.on_ground = on_block || !player.chr_ins.modules.physics.is_falling;
            self.walker.active = true;
            // Attacks, rolls, items and magic are Minecraft's; walking, jumping, dashing, ladders and
            // interact stay Elden Ring's.
            block_actions(&mut player.chr_ins.modules.action_request.disabled_action_inputs, true);
            self.written_global = Some(coords::global_from_mc(fixed));
            self.driving = true;
        } else {
            self.release(Some(&mut *player));
        }

        // Minecraft's player is wherever ours is: the walker's position while we drive, Elden Ring's
        // own otherwise (ladders, the first seconds on a map).
        let here = if self.driving { self.walker.pos } else { feet };
        let on_ground = !self.driving || self.walker.on_ground;
        sky.flags = SKY_IN_GAME | EC_FOLLOW | if on_ground { EC_ON_GROUND } else { 0 };
        sky.world_id = world_id;
        sky.teleport_seq = self.teleport_seq;
        sky.collision_epoch = self.collision.epoch;
        sky.pos_x = here[0];
        sky.pos_y = here[1];
        sky.pos_z = here[2];
        sky.yaw = yaw;
        sky.pitch = pitch;
        link.write_sky_state(&sky);

        let er_ground = collision::ground_below(&frame, player, feet[0], feet[2], feet[1] + 1.5, 6.0);
        if let Some(g) = er_ground {
            self.collision.fallback_y = Some(g);
        }
        self.collision.step(link, &frame, player, feet);
        self.blocks.drain(link);

        // Camera: F5 (or d-pad up) switches first / third person, the Minecraft way. Minecraft starts
        // in first person; the first time it drives we ask for third person once. Minecraft's
        // front-facing view is skipped (Elden Ring's camera can't look at the player from the front).
        if mc_ready {
            let f5_ready = self.last_f5.map_or(true, |t| t.elapsed() > Duration::from_millis(400));
            let want_f5 = (!self.f5_done && mc.camera_mode == 0) || mc.camera_mode == 2;
            if want_f5 && f5_ready {
                link.push_input(IN_KEY, 62, 1, 0, 0);
                link.push_input(IN_KEY, 62, 0, 0, 0);
                self.last_f5 = Some(Instant::now());
                self.f5_done = true;
                log!("F5 to Minecraft (camera mode {})", mc.camera_mode);
            }
            if mc.camera_mode != 0 {
                self.f5_done = true;
            }
        }
        let first_person = mc_ready && self.f5_done && mc.camera_mode == 0;
        if first_person != self.first_person {
            log!("camera: {}", if first_person { "first person" } else { "third person" });
            self.first_person = first_person;
        }
        if first_person {
            // Elden Ring's camera keeps its rotation (mouse / right stick) but sits at Steve's eyes,
            // with Minecraft's view bobbing and field of view.
            let t = camera::partial_tick(mc.tick_qpc, mc.tick_ms);
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            let (side, up) = camera::bob(lerp(mc.walk_dist_o, mc.walk_dist), lerp(mc.bob_o, mc.bob));
            let mut eye = frame.havok_for_mc([here[0], here[1] + EYE_HEIGHT, here[2]]);
            if let Some((m, _, _)) = cam {
                for i in 0..3 {
                    eye[i] += m[0][i] * side + m[1][i] * up;
                }
            }
            let fov = if (30.0..=130.0).contains(&mc.fov_deg) { mc.fov_deg } else { 70.0 };
            camera::set(Some(camera::Override { pos: eye, fov: fov.to_radians() }));
        } else {
            camera::set(None);
        }
        camera::apply();

        // Fighting: nearby enemies go to Minecraft as hittable stand-ins; its hits come back as damage.
        if mc_ready {
            self.actors.publish(link, &frame, player);
        } else {
            self.actors.clear(link);
        }
        self.actors.apply_hits(link);
        self.actors.player_tick(link, player, mc_ready);

        // Steve replaces the Tarnished while Minecraft drives (drawn in third person only).
        let show_steve = mc_ready && self.blocks.has_avatar() && !first_person;
        if mc_ready && (show_steve || first_person) {
            player.chr_ins.chr_flags1c5.set_enable_render(false);
            self.hid_model = true;
        } else if self.hid_model {
            player.chr_ins.chr_flags1c5.set_enable_render(true);
            self.hid_model = false;
        }
        if mc_ready {
            self.hud.update(link, mc.gui_scale);
        }
        self.hud.publish(mc_ready);
        let selection = link.read_world_entities_head().and_then(|w| {
            (w.has_selection != 0).then_some((w.sel_min, w.sel_max))
        });
        let drawn = match unsafe { RendMan::instance_mut() } {
            Ok(rend) => {
                let ez = &mut *rend.debug_ez_draw;
                let at = here;
                if show_steve {
                    self.blocks.draw_avatar(ez, &frame, at);
                }
                self.blocks.draw(ez, &frame, at, selection)
            }
            Err(_) => 0,
        };
        self.report(link, &mc, Some((feet, yaw, pitch, drawn)));
    }

    /// Hand the player back to Elden Ring.
    fn release(&mut self, player: Option<&mut eldenring::cs::PlayerIns>) {
        if !self.driving {
            return;
        }
        if let Some(p) = player {
            let phys = &mut p.chr_ins.modules.physics;
            phys.gravity_multiplier = self.saved_gravity.unwrap_or(1.0);
            phys.gravity_disabled = false;
            block_actions(&mut p.chr_ins.modules.action_request.disabled_action_inputs, false);
            if self.hid_model {
                p.chr_ins.chr_flags1c5.set_enable_render(true);
                self.hid_model = false;
            }
        }
        self.saved_gravity = None;
        self.driving = false;
        self.walker.active = false;
        self.written_global = None;
        log!("driving: handed back to Elden Ring");
    }

    fn report(&mut self, link: &link::Link, mc: &McState, here: Option<([f64; 3], f32, f32, usize)>) {
        if self.last_report.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.last_report = Instant::now();
        let where_ = match here {
            Some((f, yaw, pitch, drawn)) => format!(
                "feet ({:.2}, {:.2}, {:.2}) look ({yaw:.0}, {pitch:.0}) drawn {drawn}",
                f[0], f[1], f[2]
            ),
            None => "no player".into(),
        };
        log!(
            "frame {} | {} | walker pos ({:.2}, {:.2}, {:.2}) vel ({:.2}, {:.2}, {:.2}) ground {} rays {} blocked {} | mc alive {} pid {} flags {:#x} ack {}/{} pos ({:.2}, {:.2}, {:.2}) | driving {} | collision epoch {} columns {} walls {} rays {}/{} | render msgs {} sections {} avatar frames {} hud frames {} quads {} | actors {} hits {} | first person {}",
            self.frames,
            where_,
            self.walker.pos[0],
            self.walker.pos[1],
            self.walker.pos[2],
            self.walker.vel[0],
            self.walker.vel[1],
            self.walker.vel[2],
            self.walker.on_ground,
            self.walker.rays,
            self.walker.blocked,
            link.mc_alive(),
            link.mc_pid(),
            mc.flags,
            mc.teleport_ack,
            self.teleport_seq,
            mc.x,
            mc.y,
            mc.z,
            self.driving,
            self.collision.epoch,
            self.collision.columns_sent,
            self.collision.walls,
            self.collision.rays_hit,
            self.collision.rays_hit + self.collision.rays_missed,
            self.blocks.messages,
            self.blocks.section_count(),
            self.blocks.avatar_frames,
            self.hud.frames,
            self.hud.quad_count(),
            self.actors.mirrored,
            self.actors.hits,
            self.first_person
        );
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

#[unsafe(no_mangle)]
/// # Safety
/// Called by Windows' loader only.
pub unsafe extern "C" fn DllMain(hmodule: usize, reason: u32) -> bool {
    if reason != 1 {
        return true;
    }
    std::thread::spawn(move || {
        log::init();
        // Any panic is written to our log before the process aborts.
        std::panic::set_hook(Box::new(|info| {
            log!("PANIC: {info}");
        }));
        log!("EldenCraft {} loaded (pid {})", env!("CARGO_PKG_VERSION"), std::process::id());
        log!("steam running: {}", boot::process_running("steam.exe"));
        // Touch nothing in the game until its window is up (CS2-in-ER field note: early reflection
        // and singleton scans fail or crash while the packed executable is still starting).
        let waited = boot::wait_for_game_window(Duration::from_secs(300));
        log!("game window: {}", if waited { "up" } else { "never appeared in 5 min" });
        if !waited {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
        // Minecraft's HUD is drawn over the finished frame, pinned to the screen.
        overlay::install(hmodule);
        log!("waiting for the game's task system");
        let cs_task = match CSTaskImp::wait_for_instance(Duration::from_secs(600)) {
            Ok(t) => t,
            Err(e) => {
                log!("CSTask never came up: {e:?}");
                return;
            }
        };
        log!("task system ready");
        cs_task.run_recurring(
            |_: &FD4TaskData| {
                let mut guard = match STATE.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                guard.get_or_insert_with(State::new).frame();
            },
            CSTaskGroupIndex::ChrIns_PostPhysics,
        );
        // First person: put the camera back at Steve's eyes just before drawing.
        cs_task.run_recurring(|_: &FD4TaskData| camera::apply(), CSTaskGroupIndex::Draw_Pre);
    });
    true
}
