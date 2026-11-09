//! The wire protocol with the bun spell host: one JSON object per line.
//! Keep in sync with runtime/src/host/Protocol.ts.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WireCommand {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub mode: CommandMode,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum CommandMode {
    #[serde(rename = "view")]
    View,
    #[serde(rename = "no-view")]
    NoView,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    Callback,
    Push,
    Copy,
    Open,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WireAction {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub kind: ActionKind,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WireListItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub accessories: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub actions: Vec<WireAction>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WireMetadata {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireView {
    #[serde(rename_all = "camelCase")]
    List {
        #[serde(default)]
        placeholder: Option<String>,
        filtering: bool,
        #[serde(default)]
        empty_text: Option<String>,
        items: Vec<WireListItem>,
    },
    Detail {
        markdown: String,
        #[serde(default)]
        metadata: Vec<WireMetadata>,
        #[serde(default)]
        actions: Vec<WireAction>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToastStyle {
    Success,
    Failure,
    Info,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WireTone {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireBarIcon {
    /// An icon string: SF Symbol, emoji, or file path.
    Image { value: String },
    /// `value` is 0–1, or `None` for "no data yet".
    Ring {
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        tone: WireTone,
        #[serde(default)]
        busy: bool,
        #[serde(default)]
        pulse: bool,
        #[serde(default)]
        badge: bool,
    },
}

impl WireBarIcon {
    pub fn image_name(&self) -> Option<&str> {
        match self {
            WireBarIcon::Image { value } => Some(value),
            WireBarIcon::Ring { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MenuAction {
    #[default]
    None,
    Callback,
    Copy,
    Open,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct WireMenuItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub accessory: Option<String>,
    /// 0–1: the row is a gauge.
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub tone: WireTone,
    #[serde(default)]
    pub checked: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub action: MenuAction,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub submenu: Vec<WireMenuEntry>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireMenuEntry {
    Separator,
    Header { title: String },
    Item(WireMenuItem),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WireStatusItem {
    pub title: String,
    #[serde(default)]
    pub daemon_icon: Option<String>,
    pub icon: WireBarIcon,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub menu: Vec<WireMenuEntry>,
}

/// Host → app.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostMessage {
    Commands {
        commands: Vec<WireCommand>,
        errors: Vec<String>,
    },
    Render {
        session: String,
        depth: usize,
        view: WireView,
    },
    Pop {
        session: String,
        depth: usize,
    },
    Closed {
        session: String,
    },
    Loading {
        session: String,
        loading: bool,
    },
    Toast {
        #[serde(default)]
        session: String,
        style: ToastStyle,
        title: String,
        #[serde(default)]
        message: Option<String>,
    },
    Hud {
        message: String,
    },
    Hide,
    Clipboard {
        text: String,
    },
    Open {
        target: String,
    },
    Done {
        session: String,
        rendered: bool,
    },
    Error {
        session: String,
        title: String,
        message: String,
    },
    Status {
        daemon: String,
        item: WireStatusItem,
    },
    StatusRemoved {
        daemon: String,
    },
}

/// App → host.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppMessage {
    Run {
        session: String,
        command: String,
    },
    Query {
        session: String,
        depth: usize,
        text: String,
    },
    Action {
        session: String,
        depth: usize,
        action: String,
    },
    Pop {
        session: String,
        depth: usize,
    },
    Close {
        session: String,
    },
    Reload,
    Menu {
        daemon: String,
        item: String,
    },
    MenuOpened {
        daemon: String,
    },
    Handoff,
}

impl AppMessage {
    pub fn line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("app messages always serialize");
        line.push('\n');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(line: &str) -> HostMessage {
        serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"))
    }

    #[test]
    fn decodes_a_list_render_with_optional_fields_left_out() {
        let msg = decode(
            r#"{"type":"render","session":"s1","depth":0,"view":{"kind":"list","filtering":true,"items":[
              {"id":"a","title":"Alpha","accessories":[],"keywords":["first"],"actions":[{"id":"c","title":"Copy","kind":"copy","value":"A"}]}]}}"#,
        );
        let HostMessage::Render {
            session,
            depth,
            view:
                WireView::List {
                    placeholder,
                    filtering,
                    empty_text,
                    items,
                },
        } = msg
        else {
            panic!("not a list render")
        };
        assert_eq!(
            (session.as_str(), depth, placeholder, filtering, empty_text),
            ("s1", 0, None, true, None)
        );
        assert_eq!(items[0].actions[0].kind, ActionKind::Copy);
        assert_eq!(items[0].actions[0].value.as_deref(), Some("A"));
    }

    #[test]
    fn decodes_detail_commands_and_no_view_mode() {
        let msg = decode(
            r#"{"type":"commands","commands":[{"id":"x:uuid","title":"UUID","keywords":[],"mode":"no-view"}],"errors":[]}"#,
        );
        let HostMessage::Commands { commands, .. } = msg else {
            panic!()
        };
        assert_eq!(commands[0].mode, CommandMode::NoView);
        let msg = decode(
            r##"{"type":"render","session":"s","depth":1,"view":{"kind":"detail","markdown":"# Hi","metadata":[{"label":"L","value":"V"}],"actions":[]}}"##,
        );
        assert!(matches!(
            msg,
            HostMessage::Render {
                view: WireView::Detail { .. },
                ..
            }
        ));
    }

    #[test]
    fn decodes_a_status_item_with_a_ring_and_nested_menu() {
        let msg = decode(
            r#"{"type":"status","daemon":"insomnia","item":{"title":"Insomnia","icon":{"kind":"ring","value":null,"tone":"warning","busy":true,"pulse":false,"badge":true},
              "menu":[{"kind":"header","title":"Session"},{"kind":"separator"},
                {"kind":"item","id":"m","title":"More","tone":"normal","checked":false,"enabled":true,"action":"none","submenu":[
                  {"kind":"item","id":"c","title":"Copy","tone":"critical","checked":true,"enabled":true,"action":"copy","value":"v","submenu":[],"progress":0.4}]}]}}"#,
        );
        let HostMessage::Status { item, .. } = msg else {
            panic!()
        };
        assert_eq!(
            item.icon,
            WireBarIcon::Ring {
                value: None,
                tone: WireTone::Warning,
                busy: true,
                pulse: false,
                badge: true
            }
        );
        let WireMenuEntry::Item(more) = &item.menu[2] else {
            panic!()
        };
        let WireMenuEntry::Item(copy) = &more.submenu[0] else {
            panic!()
        };
        assert_eq!(
            (copy.action, copy.progress, copy.tone),
            (MenuAction::Copy, Some(0.4), WireTone::Critical)
        );
    }

    #[test]
    fn decodes_the_small_messages() {
        assert_eq!(decode(r#"{"type":"hide"}"#), HostMessage::Hide);
        assert_eq!(
            decode(r#"{"type":"statusRemoved","daemon":"d"}"#),
            HostMessage::StatusRemoved { daemon: "d".into() }
        );
        assert_eq!(
            decode(r#"{"type":"done","session":"s","rendered":false}"#),
            HostMessage::Done {
                session: "s".into(),
                rendered: false
            }
        );
    }

    #[test]
    fn encodes_app_messages_as_the_host_reads_them() {
        assert_eq!(AppMessage::Reload.line(), "{\"type\":\"reload\"}\n");
        assert_eq!(
            AppMessage::MenuOpened { daemon: "d".into() }.line(),
            "{\"type\":\"menuOpened\",\"daemon\":\"d\"}\n"
        );
        assert_eq!(
            AppMessage::Query {
                session: "s1".into(),
                depth: 2,
                text: "hi".into()
            }
            .line(),
            "{\"type\":\"query\",\"session\":\"s1\",\"depth\":2,\"text\":\"hi\"}\n"
        );
    }
}
