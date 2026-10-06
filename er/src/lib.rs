//! EldenCraft, Elden Ring side: loaded by ModEngine2 (external_dlls), offline with Easy Anti-Cheat
//! off. Talks to a hidden Minecraft (SkyCraft's Fabric mod) over shared memory; Minecraft runs the
//! player's movement, blocks and inventory, Elden Ring runs and draws the world.
//! Design and status: sheets/*.json. Every system here is a row in sheets/systems.json.

mod blocks;
mod collision;
mod coords;
mod input;
mod launcher;
mod link;
mod log;
mod proto;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use eldenring::cs::{CSCamera, CSTaskGroupIndex, CSTaskImp, RendMan, WorldChrMan};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::{FromStatic, SharedTaskImpExt};

use coords::Frame;
use proto::*;

/// Elden Ring teleports (grace travel, loading) move the player further than this in one frame.
const TELEPORT_METRES: f64 = 6.0;

struct State {
    link: Option<link::Link>,
    input: input::Input,
    collision: collision::Collision,
    blocks: blocks::Blocks,
    launcher: launcher::Launcher,
    teleport_seq: u32,
    world_id: u32,
    last_block: i32,
    last_global: Option<[f64; 3]>,
    written_global: Option<[f64; 3]>,
    driving: bool,
    saved_gravity: Option<f32>,
    last_report: Instant,
    frames: u64,
}

impl State {
    fn new() -> Self {
        State {
            link: link::Link::create(),
            input: input::Input::new(),
            collision: collision::Collision::new(),
            blocks: blocks::Blocks::new(),
            launcher: launcher::Launcher::new(),
            teleport_seq: 1,
            world_id: 0,
            last_block: i32::MIN,
            last_global: None,
            written_global: None,
            driving: false,
            saved_gravity: None,
            last_report: Instant::now(),
            frames: 0,
        }
    }

    fn frame(&mut self) {
        self.frames += 1;
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
        sky.viewport_w = 1920;
        sky.viewport_h = 1080;
        sky.game_hour = 12.0;
        sky.teleport_seq = self.teleport_seq;
        sky.collision_epoch = self.collision.epoch;

        // The player (absent on the title screen and while loading).
        let player_ptr = unsafe { WorldChrMan::instance_mut() }
            .ok()
            .and_then(|w| w.main_player.as_mut())
            .map(|p| &mut **p as *mut eldenring::cs::PlayerIns);
        let Some(player_ptr) = player_ptr else {
            sky.flags = SKY_LOADING;
            sky.world_id = self.world_id;
            link.write_sky_state(&sky);
            self.input.poll(link, false, false);
            self.release(None);
            self.report(link, &mc, None);
            return;
        };
        let player = unsafe { &mut *player_ptr };

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
        } else {
            // Elden Ring moved the player itself (grace travel, a cutscene, being grabbed).
            let reference = if self.driving { self.written_global } else { self.last_global };
            if let Some(r) = reference {
                let d = ((global[0] - r[0]).powi(2) + (global[1] - r[1]).powi(2) + (global[2] - r[2]).powi(2)).sqrt();
                if d > TELEPORT_METRES {
                    log!("teleport detected ({d:.1} m): Minecraft follows");
                    self.teleport_seq += 1;
                    self.collision.reset(link);
                }
            }
        }
        self.last_global = Some(global);

        // Look direction: Elden Ring's camera is authoritative (Minecraft follows it).
        let (yaw, pitch) = unsafe { CSCamera::instance() }
            .ok()
            .map(|c| {
                let m = &c.pers_cam_1.matrix;
                coords::mc_look([m.2.0, m.2.1, m.2.2])
            })
            .unwrap_or((0.0, 0.0));

        let feet = coords::mc_from_global(global);
        sky.flags = SKY_IN_GAME;
        sky.world_id = world_id;
        sky.teleport_seq = self.teleport_seq;
        sky.collision_epoch = self.collision.epoch;
        sky.pos_x = feet[0];
        sky.pos_y = feet[1];
        sky.pos_z = feet[2];
        sky.yaw = yaw;
        sky.pitch = pitch;
        link.write_sky_state(&sky);

        // Minecraft drives the player once it is in its world and has followed our last teleport.
        let mc_ready = mc_alive && (mc.flags & MC_IN_WORLD) != 0 && mc.teleport_ack == self.teleport_seq;
        if mc_ready {
            let target_mc = [mc.x, mc.y, mc.z];
            let h = frame.havok_for_mc(target_mc);
            let phys = &mut player.chr_ins.modules.physics;
            if self.saved_gravity.is_none() {
                self.saved_gravity = Some(phys.gravity_multiplier);
                log!("driving: Minecraft moves the player now (gravity was {})", phys.gravity_multiplier);
            }
            phys.position.0 = h[0];
            phys.position.1 = h[1];
            phys.position.2 = h[2];
            phys.chr_proxy_pos_update_requested = true;
            // The game's own gravity would build a hidden fall speed and kill on landing (field note).
            phys.gravity_multiplier = 0.0;
            phys.gravity_disabled = true;
            player.chr_ins.modules.fall.fall_timer = 0.0;
            // Attacks, rolls and jumps are Minecraft's now; walking stays on for the run animation.
            player.chr_ins.debug_flags.set_disabled_secondary_actions(true);
            self.written_global = Some(coords::global_from_mc(target_mc));
            self.driving = true;
        } else {
            self.release(Some(&mut *player));
        }

        self.input.poll(link, mc_ready, (mc.flags & MC_SCREEN_OPEN) != 0);
        self.collision.step(link, &frame, player, feet);
        self.blocks.drain(link);
        let selection = link.read_world_entities_head().and_then(|w| {
            (w.has_selection != 0).then_some((w.sel_min, w.sel_max))
        });
        let drawn = match unsafe { RendMan::instance_mut() } {
            Ok(rend) => {
                let ez = &mut *rend.debug_ez_draw;
                self.blocks.draw(ez, &frame, if mc_ready { [mc.x, mc.y, mc.z] } else { feet }, selection)
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
            p.chr_ins.debug_flags.set_disabled_secondary_actions(false);
        }
        self.saved_gravity = None;
        self.driving = false;
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
            "frame {} | {} | mc alive {} pid {} flags {:#x} ack {}/{} pos ({:.2}, {:.2}, {:.2}) | driving {} | collision epoch {} columns {} rays {}/{} | render msgs {} sections {}",
            self.frames,
            where_,
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
            self.collision.rays_hit,
            self.collision.rays_hit + self.collision.rays_missed,
            self.blocks.messages,
            self.blocks.section_count()
        );
    }
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

#[unsafe(no_mangle)]
/// # Safety
/// Called by Windows' loader only.
pub unsafe extern "C" fn DllMain(_hmodule: usize, reason: u32) -> bool {
    if reason != 1 {
        return true;
    }
    std::thread::spawn(|| {
        log::init();
        log!("EldenCraft {} loaded", env!("CARGO_PKG_VERSION"));
        let cs_task = match CSTaskImp::wait_for_instance(Duration::MAX) {
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
    });
    true
}
