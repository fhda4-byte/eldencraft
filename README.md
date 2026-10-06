# EldenCraft

Play Elden Ring as a Minecraft player: Minecraft's movement, blocks and inventory in the Lands Between.
Elden Ring runs its world and draws everything; a hidden Minecraft runs the player's rules. Solo, offline.

Status: early, being built. Design and progress: `sheets/` (preflight: `python3 tools/preflight.py`).

- `er/`: the Elden Ring side (Rust DLL loaded by ModEngine2).
- `minecraft-bundle/`: the bundled Minecraft (portable Prism Launcher instance with SkyCraft's Fabric mod).
- `tools/package.ps1`: builds and packages a release; CI publishes every build to the `builds` branch.

Credits: see THIRD-PARTY-NOTICES.md.
