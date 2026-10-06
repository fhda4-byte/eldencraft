# Third-party notices

EldenCraft contains no game files from Elden Ring or Minecraft. Players need their own copies of both.

- **SkyCraft** by chasmlol (MIT): the Minecraft mod in the bundled Minecraft is SkyCraft's Fabric mod
  (in `fabric/`, with small EldenCraft changes listed in fabric/README-EldenCraft.md), and the Elden Ring side speaks SkyCraft's shared-memory protocol (v11), ported to Rust.
  https://github.com/chasmlol/SkyCraft
- **fromsoftware-rs** by vswarte and contributors (MIT OR Apache-2.0): Elden Ring structures and bindings.
  https://github.com/vswarte/fromsoftware-rs
- **Prism Launcher** (GPL-3.0), unmodified portable build, starts Minecraft. https://prismlauncher.org
- **Fabric Loader / Fabric API** (Apache-2.0). https://fabricmc.net
- **ModEngine2** 2.1.0 by soulsmods (MIT), bundled unmodified in `modengine2/`: starts Elden Ring offline with
  Easy Anti-Cheat off and loads the Elden Ring side. https://github.com/soulsmods/ModEngine2

- **hudhook** by Andrea Venuta (MIT), built into eldencraft.dll: draws Minecraft's HUD over the game.
  https://github.com/veeenu/hudhook — with **Dear ImGui** (MIT, Omar Cornut), **imgui-rs** (MIT OR Apache-2.0),
  and **MinHook** (BSD-2-Clause, Tsuda Kageyu; includes Hacker Disassembler Engine, BSD-2-Clause,
  Vyacheslav Patkov). Full texts: eldencraft/LICENSE-hudhook.txt, eldencraft/LICENSE-MinHook.txt.

EldenCraft is a fan project, not affiliated with FromSoftware, Bandai Namco, Mojang or Microsoft.
Built with AI assistance (Claude).
