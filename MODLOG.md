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
