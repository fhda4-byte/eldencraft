# EldenCraft MODLOG

## Facts (verified)
- Elden Ring: D:\SteamLibrary\steamapps\common\ELDEN RING\Game, eldenring.exe FileVersion 2.7.1.0 (1.17.1).
  User's own folder has Vortex-deployed ModEngine2 + MelonLoader; Melty uses its own {managed}/modengine2 (release-2.1.0).
- Minecraft Java: %APPDATA%\.minecraft (Microsoft Store launcher). Melty's installed-games.json does NOT list minecraft-java.
- Melty game_info: ER one-click = ModEngine2 only; MC = no loader (companion Prism bundle is the SkyCraft precedent, one click: yes).
- SkyCraft (MIT, chasmlol) source cloned; protocol v11 is host-agnostic -> reuse Fabric side, rewrite host side for ER.
- Closest ER reference: knowledge/games/elden-ring/cs2-conversion... (fromsoftware-rs 0.14 Ww2710, hudhook DX12, gravity fix).

## Route
Pattern 2 passthrough: ER native Rust DLL (ModEngine2 external_dlls) <-> hidden Minecraft (SkyCraft Fabric fork) over shared memory.

## Blockers
- 2026-10-06: workspace egress allows only GitHub git. crates.io, Maven, Gradle, MS SDK downloads, melty.gg blocked.
  Melty API reached via the user's desktop browser pane (fetch from melty.gg origin).
- No shell on the user's PC (no device_bash). Build needs either workspace egress or a PC shell.

## 2026-10-06 build 0.1.0
- Build route: GitHub Actions windows-latest (repo fhda4-byte/eldencraft), every build force-pushed to `builds` branch
  with logs. First build green: er DLL 340 KB, SkyCraft fabric jar 0.1.2 @bfcaf17, Prism 11.1.1 portable.
- Melty: draft "EldenCraft" modId 1a75076f-a7d0-4f33-a485-e64aa39e4f0a (slug eldencraft-2), allowRemix true, MIT,
  linked to githubRepo. Release 0.1.0 draft (releaseId 6388484b-...), one_click_check yes.
- Uploads go browser -> raw.githubusercontent.com (CORS ok) -> signed PUT -> finish_upload.
- Save backup: %APPDATA%\EldenRing\76561198712842029\ER0000.sl2.before-eldencraft (sha256 4e0533dd...).
- Untested: everything in game. First checks: DLL log at %LOCALAPPDATA%\EldenCraft\eldencraft.log, ME2 relative
  external_dlls path, EzDraw visible in retail, axis handedness, camera forward sign, tile continuity.

## 2026-10-07 test 1 (user pressed Test): Elden Ring never started
- Melty installed ModEngine2 to %APPDATA%\Melty\mods\loaders\modengine2, not {managed}/modengine2
  ({managed} = mods\managed\eldencraft-2 for our package). Our launch path did not exist: installed 1, launched 0.
- Minecraft side fine: Prism sign-in done (user fahad132), MC 26.3 + SkyCraft 0.1.2 started hidden, waited 10 min
  for the host, quit. Melty's setup step worked ("Backend library:" seen).
- Fix (0.1.1): bundle ModEngine2 2.1.0 (MIT, sha256 8a59...00ef, same zip Melty uses) in our main zip under
  modengine2/, launch {managed}/modengine2/modengine2_launcher.exe -t er -p {game}/Game/eldenring.exe -c <abs config>.
  ME2 resolves relative external_dlls against the config folder (settings.h node_to_val).

## 2026-10-07 test 2 (0.1.2): in game, but falls out of the map; no Steve
- Fixed earlier: game closed instantly in 0.1.1 test (Steam? / early touch). 0.1.2 log: steam running true, window up,
  task system ready, mapping created, Minecraft linked (SkyCraft "linked to Skyrim", mirror world, starter + builder kit).
- Player was in m10_01 (Stormveil) and m18_00 (Stranded Graveyard), not the overworld.
- Rays from feet+40 m hit only ~20% (castle roofs/inside rock) -> empty known regions -> MC player falls -> we drove the
  Tarnished down through the floor (to y -1300). Also drove during loading (block m255_255_255_255 placeholder).
- No Steve: avatar is only sent by MC in third person; we never drew it anyway.
- 0.1.3: multi-origin rays (feet+2, +8, +20, +45) and hole filling, ground guard (never under ER floor: teleport MC
  back up), ignore m255 loading + 3 s settle, auto F5 + draw Steve avatar (EzDraw, skin texture colours) and hide the
  Tarnished (chr_flags1c5.enable_render) while Steve shows.

## 2026-10-07 tests 3-4 (0.1.3, 0.1.4)
- Steve shows (avatar via F5 + EzDraw). User: textures wrong (0.1.3 flat per-triangle) -> 0.1.4 per-texel quads.
- User wants the default Steve skin, not their account skin -> fork SkyCraft fabric into fabric/ with
  -Deldencraft.steveSkin=true (AvatarExporter.textureId maps skins/* and textures/entity/player/* to wide/steve.png).
- "Jump too high, above the map": our ground guard cast from mc.y+3 hit arches/ledges overhead and teleported the
  player up onto them (log: guard ... ground 9.76 / 14.59, then y 19-27). 0.1.5 replaces it with a void rescue
  (airborne > 2.5 s and > 30 blocks under last standing spot -> back to it).
- 0.1.6: user asked to remove the safety net entirely: no guard, no rescue. Only Elden Ring's own teleports (grace travel, loading) still move Minecraft's player.
- 0.1.6 test log: fell through at (-136, 13) to y -500 (hole); later MC walked into ER rock, ER pushed the character
  out upward (+28.6 m), our distance-based "teleport detected" made MC follow onto the roof = "jumping above the map".
- 0.1.7: walls (knee-high horizontal ray pairs per grid edge -> double-sided vertical quads in 1-block slices);
  teleports followed only after a loading screen (no distance-based detection); 1 column per frame (more rays each).
- 0.1.8: controller support through Elden Ring's own input layer (FD4PadManager in-game pad: move/jump/dash->sprint/crouch->sneak/R1 break/L1 place/triangle inventory/d-pad hotbar); camera = Elden Ring's right stick. Keyboard use in the last 1.5 s turns pad mappings off.
- user (0.1.8): can't walk up stairs or ladders; wants the Minecraft hotbar visible.
- 0.1.9: HUD = MC overlay frame -> RLE flat quads on a plane 0.3 m in front of the camera (EzDraw); viewport =
  real window client size; camera forward sign checked against the player; ladders: ER climbs (ladder state),
  MC follows after; ER interact (E / triangle) left to ER, MC inventory moved to Tab / R3; ER actions blocked via
  ActionRequest.disabled_action_inputs instead of ChrDebugFlags.disabled_secondary_actions (which blocked interact).
  Stairs: cause not yet seen; asked user for a screenshot.

## 2026-10-07 user (0.1.9): still falls (in a cave), poor graphics, hotbar moves with the camera
- Root causes: (1) Minecraft moved the player against a copied height field + wall rays, which can't describe caves,
  overhangs or stairs; (2) the HUD was drawn as a world-space plane in front of the camera (EzDraw), so it lags/moves.
- 0.2.0 movement: Minecraft physics constants run on the Elden Ring side (er/src/walk.rs) against Elden Ring's real
  collision with Havok rays at the player every frame (walls at 0.65/1.1/1.7 m, 3 lateral offsets, per axis; ground =
  highest of centre + 4 corners from a step above the feet; ceiling rays when rising; snap-down 0.55 on stairs/slopes;
  sneak stops at edges) plus Minecraft's solid blocks (REN_SOLIDS bitsets). Minecraft's player follows exactly each
  tick: Fabric follow mode (SkyState flags 1<<8 follow, 1<<9 on ground; EntityFollowMixin points Entity.move at the
  target and EntityCollideMixin returns the exact step), so fall damage, walk animation, hunger still work. Ladders:
  Elden Ring climbs, Minecraft follows its position live (no teleport needed).
- 0.2.0 HUD: hudhook 0.9.3 (DX12, MIT) draws the overlay frame as rectangles in screen pixels over the finished frame
  (ImGui background draw list). replace_texture not used: hudhook's DX12 upload leaks an upload buffer per call.
- 0.2.0 built (CI 37546744017; mixin targets move/collide confirmed in 26.3 jars) and submitted to the Melty draft (release b07eec10, one click yes). Waiting for the user's Test.

## 2026-10-07 test (0.2.0): fell out of the map again (Stranded Graveyard m18_00)
- Log: walker walked from y 6 down to the lower floor (-10.65), then at about MC (-85..-89, -40..-43) every ground ray
  missed and the walker fell to y -308. Havok rays with filter 0x02000058 don't see all of Elden Ring's floors (the
  streaming columns already showed 13/81 misses there). HUD overlay worked (hook installed, frames drawn).
- 0.2.1: Elden Ring's own physics moves the player again (no position writes, its gravity on, jump/dash/backstep
  allowed); Minecraft follows every tick (follow mode, on-ground = !physics.is_falling). Placed Minecraft blocks are
  added on top (walk::block_fix: stand on / step onto / no walking into them; gravity off while on one).
  walk.rs's ray controller is kept but unused.

## 2026-10-07 test (0.2.1): walking works; no damage to enemies; wants first/third person switch
- 0.2.2 fighting: actor table (SkyCraft protocol: seqlock, 64-byte records) from WorldChrMan.chr_inses_by_distance
  (ChrType::Npc, hp > 0, within 40 m, max 64; hostile = team_type 6/7, every team logged once); SkyCraft's server
  mirrors them as hittable SkyrimActorEntity stand-ins and sends EV_HIT_ACTOR on the event ring; we take
  ceil(mc_damage * max_hp / clamp(max_hp/25, 20, 200)) off CSChrDataModule.hp (ordinary enemy ~ zombie).
- 0.2.2 camera: F5 (keyboard) / d-pad up (pad) goes to Minecraft; Minecraft's camera_mode drives ours: 0 = first
  person (pers_cam_1 position moved to Steve's eyes at ChrIns_PostPhysics, after CameraStep; Steve not drawn),
  1 = third person; Minecraft's front view (2) is skipped with a second F5. Camera forward sign learned once in
  third person.

## 2026-10-07 test (0.2.2): hits land (log: 23 hits, soldiers 43000006 85 hp killed in 4-7 hits) but
## fighting "doesn't feel good"; first person "height/angle wrong, shakes/lags, walking and jumping feel like nothing"
- User picked (multi-choice): enemy doesn't react to hits; enemy doesn't die properly; enemies don't hit me;
  Steve's swing looks bad. First person: height/angle wrong; camera shakes or lags; walking/jumping feel wrong.
- 0.2.3 fighting: flinch = CSChrEventModule.request_animation_id 8000 on every hit that leaves HP; enemies at 0 HP
  are checked 1.5 s later (hp, death_flag, render) and logged, to see whether the game finishes them. Enemies hurt
  Steve: the Tarnished's HP losses become Minecraft damage (share of max HP x 20, IN_HURT melee, attacker =
  last_hit_by's stand-in), Tarnished HP kept full; Minecraft death -> Tarnished HP 0 (Elden Ring death).
- 0.2.3 first person: camera module writes pers_cam_1 at PostPhysics and again at Draw_Pre; eye 1.7 m;
  Minecraft's FOV (fov_deg, sprint widening) and view bobbing (walk_dist/bob interpolated by tick QPC);
  near plane 0.05 while in first person.

## 2026-10-07 test (0.2.3): "hits and guard have no weight, enemies just lose health until they die"
- Log: flinch request (8000) consumed but no visible weight; enemies at 0 HP keep death_flag false, render true
  (writing HP doesn't kill properly). Enemy hits on the Tarnished -> Minecraft hearts worked (5 hits, 20 -> 8).
- User chose (multiple choice): real Elden Ring combat.
- 0.2.4: Elden Ring's own attacks, guard, skills, rolls, magic, two-handing are allowed again (only use-item,
  gestures, Torrent blocked); no actor stand-ins sent to Minecraft (no double hits); enemy hits on the Tarnished
  still become Minecraft damage, Minecraft death still kills the Tarnished. Controls: right mouse = Elden Ring
  guard only, Minecraft place/use moved to R (keyboard) and square/UseItem (pad); pad Guard no longer places blocks;
  Minecraft inventory on pad moved to d-pad down (R3 back to lock-on); Q no longer forwarded (lock-on, not drop).
