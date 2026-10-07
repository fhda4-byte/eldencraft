//! Elden Ring enemies <-> Minecraft combat (sheets: systems.actors, player_hits_enemy).
//!
//! Every Elden Ring character near the player is published in the actor table; SkyCraft's Fabric
//! side keeps an invisible, hittable stand-in at each one. Minecraft weapons hit the stand-ins the
//! Minecraft way (crits, sharpness, cooldown) and send the damage back here, where it comes off the
//! real enemy's HP. Damage is scaled so an enemy takes about as many hits as a Minecraft mob of
//! similar strength: an ordinary soldier ~ a zombie (20 hp), bosses up to 200 Minecraft hp.

use std::collections::{HashMap, HashSet};

use eldenring::cs::{ChrIns, ChrType, PlayerIns, WorldChrMan};
use fromsoftware_shared::FromStatic;

use crate::coords::Frame;
use crate::link::Link;
use crate::log;
use crate::proto::*;

/// Characters farther than this (metres) aren't mirrored.
const RANGE: f32 = 40.0;
const MAX_MIRRORED: usize = 64;
/// Elden Ring team types that are enemies (NpcParam teamType: 6 enemy, 7 strong enemy/boss).
const HOSTILE_TEAMS: [u8; 2] = [6, 7];

pub struct Actors {
    ids: HashMap<eldenring::cs::FieldInsHandle, i32>,
    handles: HashMap<i32, eldenring::cs::FieldInsHandle>,
    next_id: i32,
    teams_seen: HashSet<(u8, i32)>,
    records: Vec<ActorRecord>,
    pub mirrored: usize,
    pub hits: u32,
}

/// Minecraft-equivalent health for an Elden Ring max HP.
fn mc_health(max_hp: i32) -> f32 {
    (max_hp as f32 / 25.0).clamp(20.0, 200.0)
}

impl Actors {
    pub fn new() -> Self {
        Actors {
            ids: HashMap::new(),
            handles: HashMap::new(),
            next_id: 1,
            teams_seen: HashSet::new(),
            records: Vec::new(),
            mirrored: 0,
            hits: 0,
        }
    }

    /// Publish the characters around the player (Minecraft coordinates).
    pub fn publish(&mut self, link: &Link, frame: &Frame, player: &PlayerIns) {
        self.records.clear();
        let me = &player.chr_ins as *const ChrIns;
        if let Ok(wcm) = unsafe { WorldChrMan::instance() } {
            for entry in wcm.chr_inses_by_distance.iter() {
                if self.records.len() >= MAX_MIRRORED || entry.distance > RANGE {
                    continue;
                }
                let chr: &ChrIns = unsafe { entry.chr_ins.as_ref() };
                if chr as *const ChrIns == me || chr.chr_type != ChrType::Npc {
                    continue;
                }
                let data = &chr.modules.data;
                if data.max_hp <= 1 || data.hp <= 0 {
                    continue;
                }
                if self.teams_seen.insert((chr.team_type, chr.npc_param_id)) && self.teams_seen.len() < 200 {
                    log!(
                        "actor: npc {} team {} hp {}/{} at {:.1} m",
                        chr.npc_param_id, chr.team_type, data.hp, data.max_hp, entry.distance
                    );
                }
                let handle = chr.field_ins_handle;
                let id = *self.ids.entry(handle).or_insert_with(|| {
                    let id = self.next_id;
                    self.next_id += 1;
                    id
                });
                self.handles.insert(id, handle);
                let phys = &chr.modules.physics;
                let p = frame.mc_for_havok([phys.position.0, phys.position.1, phys.position.2]);
                let radius = if phys.hit_radius > 0.05 { phys.hit_radius } else { 0.4 };
                let height = if phys.hit_height > 0.2 { phys.hit_height } else { 1.8 };
                let hostile = HOSTILE_TEAMS.contains(&chr.team_type);
                self.records.push(ActorRecord {
                    form_id: id,
                    flags: if hostile { ACTOR_HOSTILE } else { 0 },
                    x: p[0] as f32,
                    y: p[1] as f32,
                    z: p[2] as f32,
                    yaw: 0.0,
                    width: (radius * 2.0).clamp(0.4, 6.0),
                    height: height.clamp(0.5, 12.0),
                    health_frac: data.hp as f32 / data.max_hp as f32,
                    level: 1,
                    pad: 0,
                    name: [0; 24],
                });
            }
        }
        self.mirrored = self.records.len();
        link.write_actors(&self.records);
    }

    /// Hand nothing to Minecraft (loading screens, Minecraft not driving).
    pub fn clear(&mut self, link: &Link) {
        if self.mirrored != 0 {
            self.records.clear();
            self.mirrored = 0;
            link.write_actors(&self.records);
        }
    }

    /// Minecraft's hits come off the real enemies' HP.
    pub fn apply_hits(&mut self, link: &Link) {
        let mut events = Vec::new();
        link.drain_events(|e| events.push(e));
        if events.is_empty() {
            return;
        }
        let Ok(wcm) = (unsafe { WorldChrMan::instance_mut() }) else { return };
        for e in events {
            match e.kind {
                EV_HIT_ACTOR => {
                    let Some(handle) = self.handles.get(&e.form_id).copied() else {
                        log!("hit: unknown actor {}", e.form_id);
                        continue;
                    };
                    let Some(chr) = wcm.chr_ins_by_handle_mut(&handle) else {
                        log!("hit: actor {} is gone", e.form_id);
                        continue;
                    };
                    let data = &mut chr.modules.data;
                    let dmg = (e.a.max(0.0) * data.max_hp as f32 / mc_health(data.max_hp)).ceil() as i32;
                    let before = data.hp;
                    data.hp = (data.hp - dmg).max(0);
                    self.hits += 1;
                    log!(
                        "hit: npc {} took {} (Minecraft {:.1}) hp {} -> {} / {}",
                        chr.npc_param_id, dmg, e.a, before, chr.modules.data.hp, chr.modules.data.max_hp
                    );
                }
                EV_PLAYER_DIED => log!("Minecraft: player died"),
                other => log!("event {other} ignored"),
            }
        }
    }
}
