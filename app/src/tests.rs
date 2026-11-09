//! The bar driven headlessly through Rune's `Runtime`: keys and clicks in, host messages and
//! effects out (`Services::log` records what a live app would send or do).

use rune::{Env, Key, KeyEvent, Mods, Runtime};

use crate::fixtures;
use crate::protocol::*;
use crate::services::Services;
use crate::store::Cantrip;
use crate::views;

fn runtime(scene: fn(&mut Cantrip)) -> Runtime<Cantrip> {
    let mut state = Cantrip::new(Services::default());
    scene(&mut state);
    let mut rt = Runtime::new(
        state,
        views::view,
        views::WINDOW_SIZE,
        Env {
            system_dark: true,
            scale: 2.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<Cantrip>) {
    for _ in 0..3 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<Cantrip>, key: Key) {
    rt.key(key.into());
    settle(rt);
}

fn press_with(rt: &mut Runtime<Cantrip>, key: Key, mods: Mods) {
    rt.key(KeyEvent { key, mods });
    settle(rt);
}

fn type_text(rt: &mut Runtime<Cantrip>, text: &str) {
    for c in text.chars() {
        rt.key(Key::Text(c.to_string()).into());
    }
    settle(rt);
}

fn click(rt: &mut Runtime<Cantrip>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no element {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn titles(rt: &Runtime<Cantrip>) -> Vec<String> {
    rt.state.rows().iter().map(|r| r.title.clone()).collect()
}

fn log(rt: &mut Runtime<Cantrip>) -> Vec<String> {
    std::mem::take(&mut rt.state.services.log)
}

fn render(rt: &mut Runtime<Cantrip>, depth: usize, view: WireView) {
    let message = HostMessage::Render {
        session: "s1".into(),
        depth,
        view,
    };
    // A host message lands as an update does: through a handler with a Tx.
    rt.state.services.log.clear();
    rt.update(move |s, tx| s.receive(message, tx));
    settle(rt);
}

#[test]
fn typing_filters_the_root_search_and_arrows_move_the_selection() {
    let mut rt = runtime(fixtures::root);
    assert!(
        rt.state.field.focused,
        "the field has the keyboard from the start"
    );
    type_text(&mut rt, "cal");
    assert_eq!(rt.state.query(), "cal");
    assert_eq!(&titles(&rt)[..2], ["Calendar", "Calculator"]);
    press(&mut rt, Key::Down);
    assert_eq!(rt.state.selection(), 1);
    press_with(
        &mut rt,
        Key::Text("p".into()),
        Mods {
            ctrl: true,
            ..Mods::default()
        },
    );
    assert_eq!(rt.state.selection(), 0, "⌃P moves up");
    press(&mut rt, Key::Up);
    assert_eq!(rt.state.selection(), 0, "the top stays the top");
    assert!(rt.find("row-1").is_some());
}

#[test]
fn enter_casts_a_spell_and_escape_walks_back_out() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "hello");
    log(&mut rt);
    press(&mut rt, Key::Enter);
    assert_eq!(
        log(&mut rt),
        [r#"{"type":"run","session":"s1","command":"hello.ts:hello"}"#]
    );
    assert!(rt.state.session.is_some());
    assert_eq!(
        rt.state.field.text, "",
        "a new view starts with an empty field"
    );
    assert_eq!(rt.state.usage.get("spell:hello.ts:hello"), Some(&4));

    render(&mut rt, 0, fixtures::hello_list());
    assert_eq!(rt.state.placeholder(), "Filter greetings…");
    type_text(&mut rt, "bon");
    assert_eq!(titles(&rt), ["Bonjour"], "a filtering list filters here");
    assert!(log(&mut rt).is_empty(), "and doesn't ask the host");

    press(&mut rt, Key::Escape);
    assert_eq!(log(&mut rt), [r#"{"type":"close","session":"s1"}"#]);
    assert!(rt.state.session.is_none());
    assert_eq!(rt.state.field.text, "hello", "the root's query comes back");
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.query(), "", "escape clears the search");
    press(&mut rt, Key::Escape);
    assert_eq!(log(&mut rt), ["hide-panel"], "then hides the bar");
}

#[test]
fn the_selection_glides_from_row_to_row() {
    let mut rt = runtime(fixtures::root);
    let y = |rt: &Runtime<Cantrip>| rt.presented("selection").unwrap().0[1];
    let row_y =
        |rt: &Runtime<Cantrip>, i: usize| rt.presented(&crate::store::row_key(i)).unwrap().0[1];
    let start = y(&rt);
    assert!(
        (start - row_y(&rt, 0)).abs() < 0.5,
        "it starts under the first row"
    );
    rt.key(Key::Down.into());
    rt.key(Key::Down.into());
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    let mid = y(&rt);
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let end = y(&rt);
    assert!(
        start < mid && mid < end,
        "it travels: {start} → {mid} → {end}"
    );
    assert!(
        (end - row_y(&rt, 2)).abs() < 0.5,
        "and lands under the third row"
    );
}

#[test]
fn opening_plays_the_intro_from_a_closed_pose() {
    let mut rt = runtime(fixtures::root);
    let opacity = |rt: &Runtime<Cantrip>| rt.presented("panel").map_or(0.0, |p| p.1);
    assert!(
        (opacity(&rt) - 1.0).abs() < 0.01,
        "a fresh launch shows at rest"
    );
    rt.update(|s, _| s.hide_panel());
    settle(&mut rt);
    assert!(opacity(&rt) < 0.01, "hidden, it holds its closed pose");
    rt.update(|s, _| s.show_panel());
    settle(&mut rt);
    assert!(opacity(&rt) < 0.01, "and waits for the keyboard");
    rt.update(|s, _| s.focus_changed(true));
    assert_eq!(rt.state.intro, 1);
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    let rising = opacity(&rt);
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rising > 0.05 && rising < 0.95, "it fades in: {rising}");
    assert!((opacity(&rt) - 1.0).abs() < 0.01, "and lands open");
    assert!(rt.find("burst-1").is_some() && rt.find("stars-1").is_some());

    // The next opening replays it.
    rt.update(|s, _| s.hide_panel());
    rt.update(|s, _| s.show_panel());
    rt.update(|s, _| s.focus_changed(true));
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        opacity(&rt) < 0.95,
        "a second opening plays again: {}",
        opacity(&rt)
    );
}

#[test]
fn closing_disperses_before_the_window_hides_and_reopening_mid_way_plays_the_intro() {
    let mut rt = runtime(fixtures::root);
    // As the live app does; with no panel handle the delayed hide lands at once in the log.
    rt.state.services.live = true;
    rt.update(|s, _| s.hide_panel());
    settle(&mut rt);
    assert!(rt.state.closing, "the exit plays");
    assert_eq!(rt.state.outro, 1);
    assert!(rt.find("dust-1").is_some(), "with stardust");
    assert_eq!(log(&mut rt), ["hide-panel"]);
    rt.update(|s, _| s.finish_close(1));
    assert!(!rt.state.closing);
    assert_eq!(log(&mut rt), ["hide-panel"], "then the window hides");

    rt.update(|s, _| s.show_panel());
    rt.update(|s, _| s.focus_changed(true));
    rt.update(|s, _| s.reveal());
    rt.update(|s, _| s.hide_panel());
    assert!(rt.state.closing);
    rt.update(|s, _| s.show_panel());
    assert!(
        !rt.state.closing && rt.state.visible && rt.state.revealed,
        "reopened mid-exit, it's back"
    );
    let intro = rt.state.intro;
    rt.update(|s, _| s.finish_close(2));
    assert!(rt.state.visible, "and the late hide does nothing");
    assert_eq!(rt.state.intro, intro);
}

#[test]
fn the_selection_still_glides_after_an_opening() {
    let mut rt = runtime(fixtures::root);
    rt.update(|s, _| s.hide_panel());
    rt.update(|s, _| s.show_panel());
    rt.update(|s, _| s.focus_changed(true));
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let pill = |rt: &Runtime<Cantrip>| rt.presented("selection").unwrap();
    let start = pill(&rt).0[1];
    rt.key(Key::Down.into());
    for _ in 0..5 {
        rt.frame(1.0 / 60.0);
    }
    let (mid, opacity) = (pill(&rt).0[1], pill(&rt).1);
    assert!(
        opacity > 0.99,
        "the highlight stays lit while it moves, not replaying its entrance: {opacity}"
    );
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let end = pill(&rt).0[1];
    assert!(
        start < mid && mid < end,
        "it glides: {start} → {mid} → {end}"
    );
}

#[test]
fn moving_past_the_bottom_scrolls_smoothly_with_the_selection() {
    let mut rt = runtime(fixtures::root);
    let y = |rt: &Runtime<Cantrip>, key: &str| rt.presented(key).map(|p| p.0[1]);
    let viewport_bottom =
        |rt: &Runtime<Cantrip>| rt.presented("results").map(|p| p.0[1] + p.0[3]).unwrap();
    let mut scrolled = false;
    for _ in 0..12 {
        let before = rt.state.scroll.get().offset;
        rt.key(Key::Down.into());
        let row = crate::store::row_key(rt.state.selection());
        let (mut last_pill, mut last_row, mut worst) =
            (y(&rt, "selection").unwrap(), y(&rt, &row), 0.0f32);
        for _ in 0..60 {
            rt.frame(1.0 / 60.0);
            let (pill, now_row) = (y(&rt, "selection").unwrap(), y(&rt, &row));
            worst = worst.max((pill - last_pill).abs());
            if let (Some(a), Some(b)) = (last_row, now_row) {
                worst = worst.max((b - a).abs());
            }
            (last_pill, last_row) = (pill, now_row);
        }
        assert!(
            worst < 14.0,
            "row {} moved {worst} pt in one frame: a pop",
            rt.state.selection()
        );
        let pill = rt.presented("selection").unwrap().0;
        assert!(
            pill[1] + pill[3] <= viewport_bottom(&rt) + 0.5,
            "the selection stays in view"
        );
        scrolled |= rt.state.scroll.get().offset > before + 1.0;
    }
    assert!(scrolled, "the list scrolled to follow the selection");
}

#[test]
fn a_list_that_filters_in_the_spell_gets_each_query() {
    let mut rt = runtime(fixtures::list);
    let WireView::List {
        items,
        placeholder,
        empty_text,
        ..
    } = fixtures::hello_list()
    else {
        unreachable!()
    };
    render(
        &mut rt,
        0,
        WireView::List {
            placeholder,
            filtering: false,
            empty_text,
            items,
        },
    );
    type_text(&mut rt, "ab");
    let sent = log(&mut rt);
    assert_eq!(
        sent.last().map(String::as_str),
        Some(r#"{"type":"query","session":"s1","depth":0,"text":"ab"}"#)
    );
    assert_eq!(titles(&rt).len(), 4, "the host decides what matches");
}

#[test]
fn push_actions_wait_for_the_next_view_and_escape_pops_it() {
    let mut rt = runtime(fixtures::list);
    press(&mut rt, Key::Command('k'));
    assert!(rt.state.actions_open);
    assert!(rt.find("actions").is_some());
    press(&mut rt, Key::Down);
    log(&mut rt);
    press(&mut rt, Key::Enter);
    assert_eq!(
        log(&mut rt),
        [r#"{"type":"action","session":"s1","depth":0,"action":"details"}"#]
    );
    assert!(!rt.state.actions_open);
    assert!(
        rt.state.session.as_ref().unwrap().loading(),
        "loading while the push is on its way"
    );

    render(
        &mut rt,
        1,
        WireView::Detail {
            markdown: "# Bonjour".into(),
            metadata: Vec::new(),
            actions: Vec::new(),
        },
    );
    assert_eq!(rt.state.session.as_ref().unwrap().frames.len(), 2);
    assert!(rt.find("detail").is_some());
    press(&mut rt, Key::Escape);
    assert_eq!(log(&mut rt), [r#"{"type":"pop","session":"s1","depth":1}"#]);
    assert_eq!(rt.state.session.as_ref().unwrap().frames.len(), 1);
}

#[test]
fn command_enter_runs_the_second_action_and_copy_hides_with_a_hud() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "calendar");
    log(&mut rt);
    press_with(
        &mut rt,
        Key::Enter,
        Mods {
            cmd: true,
            ..Mods::default()
        },
    );
    assert_eq!(
        log(&mut rt),
        ["reveal /System/Applications/Calendar.app", "hide-panel"]
    );

    let mut rt = runtime(fixtures::list);
    press(&mut rt, Key::Enter);
    assert_eq!(log(&mut rt), ["hide-panel", "hud Copied to clipboard"]);
}

#[test]
fn clicking_a_row_performs_it_and_the_field_keeps_the_keyboard() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "cal");
    log(&mut rt);
    click(&mut rt, "row-1");
    assert_eq!(
        log(&mut rt),
        ["open /System/Applications/Calculator.app", "hide-panel"]
    );
    type_text(&mut rt, "x");
    assert_eq!(rt.state.query(), "calx");
}

#[test]
fn daemon_menu_items_are_searchable() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "keep awake");
    assert_eq!(titles(&rt)[0], "Keep Awake");
    log(&mut rt);
    press(&mut rt, Key::Enter);
    assert_eq!(
        log(&mut rt),
        [
            r#"{"type":"menu","daemon":"insomnia","item":"stay-awake"}"#,
            "hide-panel"
        ]
    );
}

#[test]
fn reload_reports_back_when_the_spells_arrive() {
    let mut rt = runtime(fixtures::root);
    press(&mut rt, Key::Command('r'));
    assert_eq!(log(&mut rt), [r#"{"type":"reload"}"#]);
    assert_eq!(
        rt.state.toast.as_ref().map(|t| t.title.as_str()),
        Some("Reloading grimoire…")
    );
    let message = HostMessage::Commands {
        commands: fixtures::commands(),
        errors: Vec::new(),
    };
    rt.update(move |s, tx| s.receive(message, tx));
    settle(&mut rt);
    assert_eq!(
        rt.state.toast.as_ref().map(|t| t.title.as_str()),
        Some("4 spells loaded")
    );
    assert!(rt.find("toast").is_some());
}

#[test]
fn new_spells_show_up_in_the_search_as_they_load() {
    let mut rt = runtime(|_| {});
    type_text(&mut rt, "hel");
    assert!(!titles(&rt).contains(&"Hello".to_string()));
    let message = HostMessage::Commands {
        commands: fixtures::commands(),
        errors: vec!["broken.ts: nope".into()],
    };
    rt.update(move |s, tx| s.receive(message, tx));
    settle(&mut rt);
    assert_eq!(titles(&rt)[0], "Hello");
    assert_eq!(
        rt.state.toast.as_ref().map(|t| t.title.as_str()),
        Some("1 spell(s) failed to load")
    );
}

#[test]
fn the_wand_answers_the_bar() {
    use crate::views::wand::Gesture;
    let mut rt = runtime(fixtures::root);
    press(&mut rt, Key::Up);
    press(&mut rt, Key::Down);
    assert_eq!(
        rt.state.wand.cued(),
        [Gesture::Bump { up: true }, Gesture::Flick { up: false }],
        "it shakes its head at the top of the list, then flicks down with the selection"
    );

    type_text(&mut rt, "hello");
    let spans = rt
        .state
        .wand
        .spans("hello", &crate::views::palette::arcane());
    assert_eq!(
        spans.len(),
        5,
        "every typed character is the wand's to write"
    );
    assert!(
        spans.iter().all(|s| s.color.unwrap().a == 0.0),
        "and stays hidden until its spark lands"
    );

    press(&mut rt, Key::Enter);
    assert!(rt.state.session.is_some() && rt.state.wand.casting());
    assert!(
        rt.find("wand").is_some() && rt.find("back").is_none(),
        "mid-cast, the wand keeps its place over the spell"
    );
}

/// A live window draws only when something asks for a frame. While the wand writes the query,
/// every frame must ask for the next, or the bar freezes with the characters still hidden.
#[test]
fn typing_keeps_frames_coming_until_the_text_is_written() {
    let mut rt = runtime(fixtures::root);
    rt.poll = crate::spec().poll;
    settle(&mut rt);
    let p = crate::views::palette::arcane();
    let hidden = |rt: &Runtime<Cantrip>| {
        let query = rt.state.field.text.clone();
        rt.state
            .wand
            .spans(&query, &p)
            .iter()
            .any(|s| s.color.unwrap().a < 0.01)
    };
    for c in ["t", "e", "s"] {
        rt.key(Key::Text(c.into()).into());
        // 150 ms apart, as a person types.
        for _ in 0..9 {
            rt.frame(1.0 / 60.0);
            assert!(
                rt.animating || rt.wake_deadline().is_some(),
                "a frame asks for the next while sparks fly"
            );
        }
    }
    // Only frames that were asked for: stop as soon as nothing asks.
    let mut frames = 0;
    while rt.animating || rt.wake_deadline().is_some() {
        rt.frame(1.0 / 60.0);
        frames += 1;
        if !hidden(&rt) && frames > 120 {
            break;
        }
        assert!(frames < 600, "the wand settles");
    }
    assert_eq!(rt.state.field.text, "tes");
    assert!(!hidden(&rt), "every character was written");
}
