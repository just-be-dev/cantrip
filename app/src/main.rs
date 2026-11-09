//! Cantrip: a command bar for macOS. The bar is a Rune launcher panel; spells run in a bun
//! host process (`runtime/`) that this app starts and talks to over JSON lines.
//!
//! ```sh
//! cargo run --release                                      # the app (CANTRIP_SHOW_ON_LAUNCH=1 opens the bar)
//! target/release/Cantrip --list-scenes                     # headless: fixtures to render
//! target/release/Cantrip --snap root.png --scene root      # render one without a window
//! target/release/Cantrip --scene list --tree tree.json     # the element tree, for checks
//! target/release/Cantrip --scene root --script scripts/search.json --snap s.png --at mark:end
//! ```

mod catalog;
mod config;
mod fixtures;
mod fuzzy;
mod host;
mod icons;
mod platform;
mod protocol;
mod services;
mod store;
#[cfg(test)]
mod tests;
mod views;

use std::time::{Duration, Instant};

use rune::{App, AppHandle, AppSpec, FrameOutcome, KeyEvent, OverlaySpec, ScreenEdge, Tx, WindowOptions};

use crate::config::Config;
use crate::platform::statusbar::{DaemonBar, DaemonEvent, MainCommand, MainItem, MainMenuSpec};
use crate::protocol::AppMessage;
use crate::services::Services;
use crate::store::{Builtin, Cantrip};
use crate::views::hud::{self, Hud};

/// Re-scan installed apps when the bar opens after this long.
const RESCAN_AFTER: Duration = Duration::from_secs(30);

pub fn spec() -> AppSpec<Cantrip> {
    AppSpec {
        scenes: fixtures::SCENES,
        setup: Some(|_, waker| icons::install(waker)),
        poll: Some(|s, _, _| s.tick(Instant::now())),
        on_key: Some(on_key),
        ..AppSpec::new(
            "Cantrip",
            views::WINDOW_SIZE,
            || Cantrip::new(Services::default()),
            views::view,
        )
    }
}

/// Keys while the field doesn't have focus; the field sends its own through the same handler.
fn on_key(s: &mut Cantrip, ev: &KeyEvent, tx: &mut Tx) -> bool {
    s.handle_key(ev, tx)
}

fn main() {
    rune::prewarm_gpu();
    let config = Config::load();
    let mut app = App::new(spec())
        .keep_running()
        .accessory()
        .configured_options(
            WindowOptions::launcher_panel().macos(|native| native.shadow(false)),
        )
        .on_focus(|s, focused, _| s.focus_changed(focused))
        .on_quit(|s, _| {
            if let Some(host) = &mut s.services.host {
                host.stop();
            }
        });
    let hud = app.overlay(
        OverlaySpec {
            name: "cantrip-hud",
            size: hud::SIZE,
            edge: ScreenEdge::Bottom,
            margin: 160.0,
            align: rune::Align::Center,
            inset: 0.0,
            interactive: false,
            view: hud::view,
        },
        Hud::default(),
    );
    hud.hide();
    let handle = app.handle();
    let (me, ready) = (handle.clone(), handle.clone());
    let started = std::cell::Cell::new(false);
    let revealer = handle.clone();
    let ready_config = config.clone();
    app.init(move || {
        let live = rune::tooling::headless_run().is_none();
        let services = Services {
            live,
            panel: live.then(|| me.clone()),
            hud: live.then(|| hud.clone()),
            keep_open: std::env::var_os("CANTRIP_KEEP_OPEN").is_some(),
            ..Services::default()
        };
        Cantrip::new(services)
    })
    // The first frame drawn after the panel has the keyboard is the first one anyone sees:
    // start the intro there.
    .on_frame_encoded(move |frame| {
        if frame.outcome() == FrameOutcome::Drawn && store::AWAITING_FRAME.swap(false, std::sync::atomic::Ordering::Relaxed) {
            revealer.update(|s, _| {
                if s.visible && !s.revealed {
                    s.reveal();
                }
            });
        }
    })
    .global_shortcut(&config.hotkey, |s, tx| {
        s.toggle_panel();
        // The intro starts when the panel has the keyboard; this wakes the fallback in `tick`.
        tx.wake_after(store::REVEAL_FALLBACK + Duration::from_millis(50));
    })
    .on_ready(move |_| {
        if started.replace(true) {
            return;
        }
        let (handle, config) = (ready.clone(), ready_config.clone());
        ready.update(move |s, _| start(s, config, handle));
    })
    .run();
}

/// Everything that needs a running app: the host, the menu bar, the app catalog.
fn start(s: &mut Cantrip, config: Config, handle: AppHandle<Cantrip>) {
    if !s.services.live {
        return;
    }
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };

    let mut host = host::Host::new(config.clone(), handle.clone());
    host.start();
    s.services.host = Some(host);

    let bar_handle = handle.clone();
    s.services.daemon_bar = Some(DaemonBar::new(move |daemon, event| {
        let daemon = daemon.to_string();
        match event {
            DaemonEvent::Chose(item) => {
                bar_handle.update(move |s, tx| s.choose_menu_item(&daemon, &item, tx))
            }
            DaemonEvent::Opened => {
                bar_handle.update(move |s, _| s.services.send(AppMessage::MenuOpened { daemon }))
            }
        }
    }));

    let builtin = |command, b: Builtin, key| (command, b.title(), b.detail(), b.icon(), key);
    let menu = MainMenuSpec {
        shortcut: config.menu_shortcut(),
        builtins: vec![
            builtin(MainCommand::OpenGrimoire, Builtin::OpenGrimoire, ""),
            builtin(MainCommand::OpenJournal, Builtin::OpenJournal, ""),
            builtin(MainCommand::ReloadGrimoire, Builtin::ReloadGrimoire, "r"),
            builtin(MainCommand::RestartHost, Builtin::RestartHost, ""),
        ],
    };
    let menu_handle = handle.clone();
    s.services.main_item = Some(MainItem::new(mtm, menu, move |command| {
        menu_handle.update(move |s, tx| main_command(s, command, tx))
    }));

    platform::login::enable_on_first_run();
    rescan(s, &handle);

    if std::env::var_os("CANTRIP_SHOW_ON_LAUNCH").is_some() {
        s.show_panel();
    }
}

/// Pick up newly installed apps, off the UI thread.
fn rescan(s: &mut Cantrip, handle: &AppHandle<Cantrip>) {
    s.last_scan = Some(Instant::now());
    let handle = handle.clone();
    std::thread::spawn(move || {
        let apps = catalog::scan();
        handle.update(move |s, _| {
            s.apps = apps;
            s.changed();
        });
    });
}

fn main_command(s: &mut Cantrip, command: MainCommand, tx: &mut Tx) {
    match command {
        MainCommand::OpenPanel => s.show_panel(),
        MainCommand::OpenGrimoire => s.perform_builtin(Builtin::OpenGrimoire, tx),
        MainCommand::OpenJournal => s.perform_builtin(Builtin::OpenJournal, tx),
        MainCommand::ReloadGrimoire => s.perform_builtin(Builtin::ReloadGrimoire, tx),
        MainCommand::RestartHost => s.perform_builtin(Builtin::RestartHost, tx),
        MainCommand::ToggleLogin => {
            platform::login::toggle();
            if let Some(item) = &s.services.main_item {
                item.refresh_login();
            }
        }
        MainCommand::Quit => tx.quit(),
    }
}

impl Cantrip {
    /// The bar is opening: re-scan apps if it's been a while.
    pub fn opening(&mut self) {
        let stale = self.last_scan.is_none_or(|t| t.elapsed() > RESCAN_AFTER);
        if stale
            && self.services.live
            && let Some(panel) = self.services.panel.clone()
        {
            rescan(self, &panel);
        }
    }
}
