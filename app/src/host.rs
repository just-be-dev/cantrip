//! Runs the bun spell host as a child process and talks JSON lines over stdio.
//! Messages land in the app's state through `AppHandle::update`, on the UI thread.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rune::AppHandle;

use crate::config::{Config, home};
use crate::protocol::{AppMessage, HostMessage};
use crate::store::Cantrip;

pub fn log_path() -> PathBuf {
    home().join("Library/Logs/Cantrip/host.log")
}

pub struct Host {
    config: Config,
    app: AppHandle<Cantrip>,
    /// Bumped on every start, so a dead process's late events are ignored.
    generation: u64,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    pid: Option<u32>,
    quitting: bool,
    restart_delay: Duration,
    started_at: Option<Instant>,
}

impl Host {
    pub fn new(config: Config, app: AppHandle<Cantrip>) -> Self {
        Host {
            config,
            app,
            generation: 0,
            stdin: None,
            pid: None,
            quitting: false,
            restart_delay: Duration::from_millis(500),
            started_at: None,
        }
    }

    pub fn start(&mut self) {
        let app = self.app.clone();
        let Some(bun) = find_bun(&self.config) else {
            eprintln!("Cantrip: could not find bun. Set \"bun\" in ~/.config/cantrip/config.json");
            let message = HostMessage::Error {
                session: String::new(),
                title: "bun not found".into(),
                message: "Install bun, or set \"bun\" in ~/.config/cantrip/config.json.".into(),
            };
            app.update(move |s, tx| s.receive(message, tx));
            return;
        };
        let Some(entry) = find_host_entry() else {
            eprintln!("Cantrip: could not find runtime/src/host/main.ts");
            return;
        };
        let mut command = Command::new(&bun);
        command
            .arg(&entry)
            .current_dir(entry.parent().unwrap_or(Path::new("/")))
            .env("PATH", path_env(&bun))
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(open_log().map_or(Stdio::null(), Stdio::from));
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Cantrip: failed to start host: {e}");
                return;
            }
        };
        self.generation += 1;
        let generation = self.generation;
        eprintln!(
            "Cantrip: started host {} {} (pid {})",
            bun.display(),
            entry.display(),
            child.id()
        );
        self.pid = Some(child.id());
        self.stdin = child.stdin.take().map(|s| Arc::new(Mutex::new(s)));
        self.started_at = Some(Instant::now());

        let stdout = child.stdout.take().expect("piped stdout");
        let reader_app = app.clone();
        std::thread::Builder::new()
            .name("cantrip-host-reader".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<HostMessage>(&line) {
                        Ok(message) => reader_app.update(move |s, tx| s.receive(message, tx)),
                        Err(e) => eprintln!(
                            "Cantrip: undecodable host line: {} ({e})",
                            line.chars().take(300).collect::<String>()
                        ),
                    }
                }
            })
            .expect("host reader thread");
        std::thread::Builder::new()
            .name("cantrip-host-waiter".into())
            .spawn(move || {
                let status = wait(child);
                app.update(move |s, tx| {
                    if let Some(host) = s
                        .services
                        .host
                        .as_mut()
                        .filter(|h| h.generation == generation)
                    {
                        host.did_terminate(status);
                        s.host_restarted(tx);
                    }
                });
            })
            .expect("host waiter thread");
    }

    pub fn send(&self, message: &AppMessage) {
        let Some(stdin) = &self.stdin else { return };
        if let Err(e) = stdin.lock().unwrap().write_all(message.line().as_bytes()) {
            eprintln!("Cantrip: write to host failed: {e}");
        }
    }

    /// Restart the host. Daemons are told it's a restart first, so they can hand their state
    /// over (Insomnia keeps its session) instead of tearing down. Every running spell stops.
    pub fn restart(&self) {
        self.send(&AppMessage::Handoff);
        let pid = self.pid;
        // Give the host a moment to read the handoff before SIGTERM arrives; the waiter starts
        // a new one.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(pid) = pid {
                unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            }
        });
    }

    /// Quit: close stdin (the host's cue to shut down and run daemon finalizers), then SIGTERM.
    pub fn stop(&mut self) {
        self.quitting = true;
        self.stdin = None;
        if let Some(pid) = self.pid.take() {
            unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        }
    }

    fn did_terminate(&mut self, status: String) {
        self.stdin = None;
        self.pid = None;
        if self.quitting {
            return;
        }
        // A host that stayed up for a while resets the backoff.
        if self
            .started_at
            .is_some_and(|t| t.elapsed() > Duration::from_secs(60))
        {
            self.restart_delay = Duration::from_millis(500);
        }
        let delay = self.restart_delay;
        eprintln!("Cantrip: host exited ({status}); restarting in {delay:?}");
        self.restart_delay = (self.restart_delay * 2).min(Duration::from_secs(30));
        let app = self.app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            app.update(|s, _| {
                if let Some(host) = s
                    .services
                    .host
                    .as_mut()
                    .filter(|h| !h.quitting && h.pid.is_none())
                {
                    host.start();
                }
            });
        });
    }
}

fn wait(mut child: Child) -> String {
    match child.wait() {
        Ok(status) => status.to_string(),
        Err(e) => e.to_string(),
    }
}

fn open_log() -> Option<std::fs::File> {
    let path = log_path();
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn find_bun(config: &Config) -> Option<PathBuf> {
    let home = home();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(configured) = &config.bun {
        candidates.push(crate::config::expand_tilde(configured));
    }
    if let Some(env) = std::env::var_os("CANTRIP_BUN") {
        candidates.push(env.into());
    }
    // The project's pinned bun, recorded by scripts/bundle.sh at build time.
    if let Some(pinned) = crate::platform::info_string("CantripBun") {
        candidates.push(pinned.into());
    }
    candidates.extend([
        home.join(".bun/bin/bun"),
        home.join(".local/share/mise/shims/bun"),
        "/opt/homebrew/bin/bun".into(),
        "/usr/local/bin/bun".into(),
    ]);
    if let Some(found) = candidates.into_iter().find(|p| is_executable(p)) {
        return Some(found);
    }
    // Last resort: ask a login shell.
    let out = Command::new("/bin/zsh")
        .args(["-lc", "command -v bun"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then(|| path.into())
}

/// The host entry point: `$CANTRIP_RUNTIME`, the app bundle's Resources, or (for `cargo run`
/// during development) a `runtime/` dir above the executable.
pub fn find_host_entry() -> Option<PathBuf> {
    let relative = "src/host/main.ts";
    if let Some(env) = std::env::var_os("CANTRIP_RUNTIME") {
        return Some(PathBuf::from(env).join(relative));
    }
    if let Some(resources) = crate::platform::bundle_resources() {
        let bundled = resources.join("runtime").join(relative);
        if bundled.exists() {
            return Some(bundled);
        }
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.ancestors()
        .skip(1)
        .map(|dir| dir.join("runtime").join(relative))
        .find(|p| p.exists())
}

/// GUI apps get a bare PATH. Give spells the usual tool locations so `git`, `brew`, `gh`, etc.
/// resolve like they do in a terminal.
fn path_env(bun: &Path) -> String {
    let home = home();
    let mut entries: Vec<String> = vec![
        bun.parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        home.join(".bun/bin").to_string_lossy().into_owned(),
        home.join(".local/bin").to_string_lossy().into_owned(),
        home.join(".local/share/mise/shims")
            .to_string_lossy()
            .into_owned(),
        "/opt/homebrew/bin".into(),
        "/opt/homebrew/sbin".into(),
        "/usr/local/bin".into(),
    ];
    let existing = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin:/usr/sbin:/sbin".into());
    entries.extend(existing.split(':').map(str::to_string));
    let mut seen = Vec::new();
    for e in entries {
        if !e.is_empty() && !seen.contains(&e) {
            seen.push(e);
        }
    }
    seen.join(":")
}
