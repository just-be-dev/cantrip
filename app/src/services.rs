//! What the state reaches outside itself: the host, the panel window, the HUD, the menu bar,
//! NSWorkspace and user defaults. Headless runs and tests have none of them (`live` is false),
//! and record what they would have done in `log` instead.

use crate::views::theme::Theme;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rune::{AppHandle, DisplayTarget, OverlayHandle, OverlayPlacement, ScreenEdge};

use crate::host::Host;
use crate::platform::statusbar::{DaemonBar, MainItem};
use crate::protocol::{AppMessage, WireStatusItem};
use crate::store::Cantrip;
use crate::views::hud::Hud;

/// How long a HUD stays up before it fades.
const HUD_LIFE: Duration = Duration::from_millis(1600);

#[derive(Default)]
pub struct Services {
    pub live: bool,
    pub host: Option<Host>,
    pub panel: Option<AppHandle<Cantrip>>,
    pub hud: Option<OverlayHandle<Hud>>,
    pub main_item: Option<MainItem>,
    pub daemon_bar: Option<DaemonBar>,
    /// Set CANTRIP_KEEP_OPEN=1 to keep the bar up when it loses focus (handy while debugging spells).
    pub keep_open: bool,
    pub hud_generation: Arc<AtomicU64>,
    /// Effects a live app would have performed, for tests and headless runs.
    pub log: Vec<String>,
}

impl Services {
    pub fn send(&mut self, message: AppMessage) {
        match &self.host {
            Some(host) => host.send(&message),
            None => self.log.push(message.line().trim_end().to_string()),
        }
    }

    pub fn restart_host(&mut self) {
        match &self.host {
            Some(host) => host.restart(),
            None => self.log.push("restart-host".into()),
        }
    }

    pub fn open(&mut self, target: &str) {
        if self.live {
            crate::platform::open(target)
        } else {
            self.log.push(format!("open {target}"))
        }
    }

    pub fn reveal(&mut self, path: &str) {
        if self.live {
            crate::platform::reveal(path)
        } else {
            self.log.push(format!("reveal {path}"))
        }
    }

    pub fn hide_panel(&mut self) {
        match &self.panel {
            Some(panel) => panel.hide(),
            None => self.log.push("hide-panel".into()),
        }
    }

    /// Hide the window once the exit has played, if it's still closing then.
    pub fn hide_panel_after(&mut self, delay: Duration, outro: u64) {
        let Some(panel) = self.panel.clone() else {
            return self.hide_panel();
        };
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            panel.update(move |s, _| s.finish_close(outro));
        });
    }

    /// Centred across, high on the screen with the pointer, like Spotlight.
    pub fn show_panel(&mut self) {
        let Some(panel) = &self.panel else {
            return self.log.push("show-panel".into());
        };
        panel.place(
            DisplayTarget::UnderPointer,
            OverlayPlacement::Edge {
                edge: ScreenEdge::Top,
                margin: 150.0 - crate::views::MARGIN,
                align: rune::Align::Center,
                inset: 0.0,
            },
        );
        panel.show();
        panel.focus();
    }

    /// A small message that fades in over the screen and goes away by itself.
    pub fn hud(&mut self, message: &str, theme: Theme) {
        let Some(hud) = &self.hud else {
            return self.log.push(format!("hud {message}"));
        };
        let message = message.to_string();
        hud.update(move |h, _| {
            h.message = message;
            h.theme = theme;
            h.shown += 1;
        });
        hud.set_display(DisplayTarget::UnderPointer);
        hud.fade_in(Duration::from_millis(120));
        let generation = self.hud_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (hud, current) = (hud.clone(), self.hud_generation.clone());
        std::thread::spawn(move || {
            std::thread::sleep(HUD_LIFE);
            if current.load(Ordering::SeqCst) == generation {
                hud.fade_out(Duration::from_millis(250));
            }
        });
    }

    pub fn status(&mut self, daemon: &str, item: &WireStatusItem) {
        if let Some(bar) = &mut self.daemon_bar {
            bar.update(daemon, item);
        }
    }

    pub fn status_removed(&mut self, daemon: &str) {
        if let Some(bar) = &mut self.daemon_bar {
            bar.remove(daemon);
        }
    }

    pub fn remove_all_status(&mut self) {
        if let Some(bar) = &mut self.daemon_bar {
            bar.remove_all();
        }
    }

    pub fn load_usage(&self) -> HashMap<String, i64> {
        if self.live {
            crate::platform::load_usage()
        } else {
            HashMap::new()
        }
    }

    pub fn save_usage(&mut self, usage: &HashMap<String, i64>) {
        if self.live {
            crate::platform::save_usage(usage)
        }
    }

    /// The theme last chosen in the bar, if any.
    pub fn load_theme(&self) -> Option<Theme> {
        if self.live {
            crate::platform::load_string("theme").and_then(|id| Theme::parse(&id))
        } else {
            None
        }
    }

    pub fn save_theme(&mut self, theme: Theme) {
        if self.live {
            crate::platform::save_string("theme", theme.id());
        } else {
            self.log.push(format!("theme {}", theme.id()));
        }
    }
}
