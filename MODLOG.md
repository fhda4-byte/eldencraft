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
