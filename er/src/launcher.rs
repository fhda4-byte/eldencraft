//! Starts the bundled Minecraft (portable Prism Launcher, instance "EldenCraft") when no Minecraft
//! with the mod answers on the shared memory (sheet: systems.launcher). Minecraft quits by itself
//! when this Elden Ring process exits (SkyCraft Fabric mod watches Header.host_pid).

use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::log;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const GRACE: Duration = Duration::from_secs(6);
const RETRY: Duration = Duration::from_secs(90);

pub struct Launcher {
    started: Instant,
    last_attempt: Option<Instant>,
}

impl Launcher {
    pub fn new() -> Self {
        Launcher { started: Instant::now(), last_attempt: None }
    }

    pub fn tick(&mut self, mc_alive: bool) {
        if mc_alive || self.started.elapsed() < GRACE {
            return;
        }
        if let Some(t) = self.last_attempt {
            if t.elapsed() < RETRY {
                return;
            }
        }
        self.last_attempt = Some(Instant::now());
        let prism = crate::log::data_dir().join("Prism").join("prismlauncher.exe");
        if !prism.exists() {
            log!("launcher: {} not found (Melty sets it up before the first start)", prism.display());
            return;
        }
        match Command::new(&prism)
            .args(["--launch", "EldenCraft"])
            .current_dir(prism.parent().unwrap())
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
        {
            Ok(child) => log!("launcher: started Minecraft (Prism pid {})", child.id()),
            Err(e) => log!("launcher: couldn't start {}: {e}", prism.display()),
        }
    }
}
