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
    // The wand comes first; the bar swells out of it at `intro::GROW`.
    for _ in 0..4 {
        rt.frame(1.0 / 60.0);
    }
    assert!(opacity(&rt) < 0.01, "the bar waits for the wand to cast it");
    for _ in 0..6 {
        rt.frame(1.0 / 60.0);
    }
    let rising = opacity(&rt);
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    assert!(rising > 0.05 && rising < 0.95, "it fades in: {rising}");
    assert!((opacity(&rt) - 1.0).abs() < 0.01, "and lands open");
    assert!(
        rt.find("burst-1").is_none(),
        "Sigil inscribes a sigil instead of a burst"
    );
    rt.update(|s, _| s.set_theme(crate::views::theme::Theme::Arcane));
    settle(&mut rt);
    assert!(rt.find("burst-1").is_some(), "Arcane's burst");

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
    assert!(
        rt.find("binding-1").is_some(),
        "Sigil binds the bar in a sigil"
    );
    rt.update(|s, _| s.set_theme(crate::views::theme::Theme::Arcane));
    rt.frame(1.0 / 60.0);
    assert!(
        rt.find("dust-1").is_some(),
        "Arcane disperses into stardust"
    );
    rt.update(|s, _| s.set_theme(crate::views::theme::Theme::Sigil));
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
        entries: Vec::new(),
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
        entries: Vec::new(),
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
        .spans("hello", &crate::views::theme::Theme::Arcane.palette());
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
    let p = crate::views::theme::Theme::Arcane.palette();
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

#[test]
fn opening_the_wand_comes_first_and_casts_the_bar() {
    let mut rt = runtime(fixtures::opening);
    rt.poll = crate::spec().poll;
    rt.update(|s, _| s.focus_changed(true));
    let panel = |rt: &Runtime<Cantrip>| rt.presented("shell").map(|p| (p.0, p.1));
    for _ in 0..5 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        rt.find("hero").is_some(),
        "the wand is up first, over the window"
    );
    let (rect, _) = panel(&rt).unwrap();
    assert!(rect[2] < 40.0, "the bar is still a speck: {rect:?}");
    for _ in 0..20 {
        rt.frame(1.0 / 60.0);
    }
    let (rect, opacity) = panel(&rt).unwrap();
    assert!(
        opacity > 0.9 && rect[2] > 600.0,
        "the bar has swelled out: {rect:?}"
    );
    for _ in 0..12 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        !rt.state.wand.flying(),
        "the wand is home in the search row by 0.6 s"
    );
    for _ in 0..12 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        rt.find("hero").is_none(),
        "and the sigil has faded by 0.8 s"
    );
}

/// Deleting the last of the query, its ghost fades where it stood; the placeholder waits for it
/// instead of showing through, then fades in.
#[test]
fn the_placeholder_waits_for_deleted_text_to_fade() {
    let mut rt = runtime(fixtures::root);
    rt.poll = crate::spec().poll;
    settle(&mut rt);
    type_text(&mut rt, "hi");
    for _ in 0..90 {
        rt.frame(1.0 / 60.0);
    }
    let alpha = |rt: &Runtime<Cantrip>| rt.state.wand.placeholder_alpha();
    assert_eq!(alpha(&rt), 1.0);
    for _ in 0..2 {
        rt.key(Key::Backspace.into());
    }
    // The edits land in the next frame, after its poll: the ghost hasn't started fading yet.
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.field.text, "");
    assert_eq!(alpha(&rt), 0.0, "not even for the frame the edit lands in");
    for _ in 0..12 {
        rt.frame(1.0 / 60.0);
        assert!(alpha(&rt) < 0.05, "hidden while the ghost fades");
    }
    for _ in 0..30 {
        rt.frame(1.0 / 60.0);
    }
    assert_eq!(alpha(&rt), 1.0, "then it's back");
}

#[test]
fn one_theme_command_previews_each_theme_as_you_scroll() {
    use crate::views::theme::Theme;
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "theme");
    assert_eq!(titles(&rt)[0], "Change Theme");
    assert!(
        !titles(&rt)
            .iter()
            .any(|t| t.ends_with(" Theme") && t != "Change Theme"),
        "one command, not one per theme"
    );
    press(&mut rt, Key::Enter);
    assert!(rt.state.picker.is_some());
    assert_eq!(titles(&rt), ["Sigil", "Arcane"]);
    assert_eq!(
        rt.state.query(),
        "",
        "the picker starts with its own empty query"
    );
    assert_eq!(rt.state.selection(), 0, "on the theme in use");
    assert_eq!(rt.state.section(), Some("Themes"));
    assert!(rt.find("picker").is_some());

    // Moving the highlight wears the theme, without saving it.
    press(&mut rt, Key::Down);
    assert_eq!(rt.state.theme, Theme::Arcane);
    press(&mut rt, Key::Up);
    assert_eq!(rt.state.theme, Theme::Sigil);
    press(&mut rt, Key::Down);
    assert!(!log(&mut rt).iter().any(|l| l.starts_with("theme ")));

    // Esc puts the old one back, and the root query.
    press(&mut rt, Key::Escape);
    assert!(rt.state.picker.is_none());
    assert_eq!(rt.state.theme, Theme::Sigil);
    assert_eq!(rt.state.query(), "theme");
    assert_eq!(rt.state.field.text, "theme");

    // ↵ keeps the highlighted one.
    press(&mut rt, Key::Enter);
    type_text(&mut rt, "arc");
    assert_eq!(titles(&rt), ["Arcane"]);
    assert_eq!(
        rt.state.theme,
        Theme::Arcane,
        "filtering previews the first match"
    );
    press(&mut rt, Key::Enter);
    assert!(rt.state.picker.is_none());
    assert_eq!(rt.state.theme, Theme::Arcane);
    assert!(log(&mut rt).contains(&"theme arcane".to_string()));
    assert_eq!(rt.state.query(), "", "back to a fresh search");

    // Reopened, it starts on the theme in use, and closing the bar cancels a preview.
    rt.update(|s, _| s.open_theme_picker());
    assert_eq!(rt.state.selection(), 1);
    press(&mut rt, Key::Up);
    assert_eq!(rt.state.theme, Theme::Sigil);
    rt.update(|s, _| s.hide_panel());
    assert!(rt.state.picker.is_none());
    assert_eq!(rt.state.theme, Theme::Arcane);
}

#[test]
fn the_grimoire_opens_as_a_book_to_leaf_through_and_cast_from() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "open grimoire");
    assert_eq!(titles(&rt)[0], "Open Grimoire");
    press(&mut rt, Key::Enter);
    assert!(rt.state.book.is_some());
    assert!(rt.find("book").is_some(), "the spread is drawn");
    assert!(
        rt.find("page-curve").is_some(),
        "its pages are redrawn curved"
    );
    assert_eq!(
        rt.state.query(),
        "",
        "the book starts with its own empty query"
    );
    assert_eq!(rt.state.placeholder(), "Search the grimoire…");
    assert_eq!(rt.state.section(), None, "the index heads its own column");
    assert!(rt.find("index-column").is_some(), "beside the book");
    assert_eq!(
        titles(&rt),
        ["Generate UUID", "Git Branches", "Hello", "Party Mode"],
        "every spell, alphabetical, and nothing else"
    );
    assert_eq!(rt.state.rows()[2].accessories, ["3"], "each with its folio");
    assert!(
        rt.find("entry-spell:uuid.ts:uuid").is_some(),
        "its entry on the left"
    );
    assert!(
        rt.find("source-spell:uuid.ts:uuid").is_some(),
        "its incantation on the right"
    );

    // The arrows turn the pages; the spread beneath is the one turned from until they land.
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Down);
    assert_eq!(rt.state.selection(), 2);
    assert!(
        rt.find("entry-spell:uuid.ts:uuid").is_some(),
        "still in the air"
    );
    rt.update(|s, tx| {
        for t in [10.0, 11.0, 11.05] {
            s.animate_book(t, tx);
        }
    });
    settle(&mut rt);
    assert!(rt.find("entry-spell:hello.ts:hello").is_some());
    assert!(rt.find("source-spell:hello.ts:hello").is_some());

    // Typing searches it; folios keep their place in the book.
    type_text(&mut rt, "party");
    assert_eq!(titles(&rt), ["Party Mode"]);
    assert_eq!(rt.state.rows()[0].accessories, ["4"]);
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.query(), "", "Esc clears the search first");
    assert!(rt.state.book.is_some());

    // ↵ casts the open entry; Back from the spell comes back to the book.
    log(&mut rt);
    press(&mut rt, Key::Enter);
    assert_eq!(
        log(&mut rt),
        [
            "hide-panel",
            r#"{"type":"run","session":"s1","command":"uuid.ts:uuid"}"#
        ],
        "a spell with no view is cast at once"
    );
    rt.update(|s, _| s.show_panel());
    press(&mut rt, Key::Down);
    press(&mut rt, Key::Enter);
    assert!(rt.state.session.is_some());
    press(&mut rt, Key::Escape);
    assert!(rt.state.session.is_none());
    assert!(rt.state.book.is_some(), "back on the book's page");
    assert_eq!(titles(&rt)[0], "Generate UUID");

    // ⌘↵ shows the spell's file; Esc closes the book, back to the root search's query.
    press_with(
        &mut rt,
        Key::Enter,
        Mods {
            cmd: true,
            ..Mods::default()
        },
    );
    assert!(log(&mut rt).iter().any(|l| l.starts_with("reveal ")));
    rt.update(|s, _| s.show_panel());
    press(&mut rt, Key::Escape);
    assert!(rt.state.book.is_none());
    assert_eq!(rt.state.query(), "open grimoire");
    assert_eq!(rt.state.field.text, "open grimoire");
}

#[test]
fn disabled_spells_are_enabled_from_the_book() {
    let mut rt = runtime(|s| {
        fixtures::root(s);
        s.entries = fixtures::dormant();
    });
    assert!(
        !titles(&rt).contains(&"Insomnia".to_string()),
        "a disabled spell isn't in the root search"
    );
    rt.update(|s, _| s.open_book());
    settle(&mut rt);
    assert_eq!(
        titles(&rt),
        [
            "Claude Usage",
            "Generate UUID",
            "Git Branches",
            "Hello",
            "Insomnia",
            "Party Mode"
        ],
        "the book lists every spell, enabled or not"
    );
    type_text(&mut rt, "insomnia");
    assert_eq!(titles(&rt), ["Insomnia"]);
    assert_eq!(
        crate::views::book::row_state(&rt.state.rows()[0]),
        crate::views::book::RowState::Dormant
    );
    log(&mut rt);
    press(&mut rt, Key::Enter);
    assert_eq!(
        log(&mut rt),
        [r#"{"type":"setEnabled","entry":"insomnia","enabled":true}"#],
        "↵ enables it, and the bar stays on the book"
    );
    assert!(rt.state.book.is_some());
    assert_eq!(
        rt.state.toast.as_ref().map(|t| t.title.as_str()),
        Some("Insomnia enabled")
    );
    assert_eq!(
        crate::views::book::row_state(&rt.state.rows()[0]),
        crate::views::book::RowState::Daemon,
        "a daemon-only spell, once enabled, keeps watch"
    );
    // The last action disables it again.
    let disable = rt.state.rows()[0].actions.last().cloned().unwrap();
    assert_eq!(disable.title, "Disable Spell");
    rt.update(move |s, tx| s.perform(disable, tx));
    assert_eq!(
        log(&mut rt),
        [r#"{"type":"setEnabled","entry":"insomnia","enabled":false}"#]
    );
}

#[test]
fn the_grimoire_folder_is_still_a_command_away() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "grimoire in finder");
    assert_eq!(titles(&rt)[0], "Show Grimoire in Finder");
    log(&mut rt);
    press(&mut rt, Key::Enter);
    let dir = crate::config::Config::grimoire_directory();
    assert_eq!(
        log(&mut rt),
        [
            format!("open {}", dir.to_string_lossy()),
            "hide-panel".into()
        ]
    );
}

#[test]
fn moving_through_the_book_turns_leaves_on_the_frame_clock() {
    let mut book = crate::views::book::Book::new();
    book.select(1);
    book.select(0);
    let mut rt = runtime(fixtures::root);
    rt.update(move |s, tx| {
        let mut book = book;
        assert!(
            book.leaves.frame(0.0, || false, tx),
            "the cover and two leaves in the air"
        );
        assert!(book.leaves.frame(0.3, || false, tx));
        assert!(
            book.leaves.frame(2.0, || false, tx),
            "drawn once lying exactly as they land"
        );
        assert!(
            book.leaves.frame(2.05, || false, tx),
            "then gone, which the view must draw"
        );
        assert!(
            book.leaves.frame(2.4, || false, tx),
            "while the writing seeps into the pages"
        );
        assert!(book.leaves.frame(2.7, || false, tx));
        assert!(!book.leaves.frame(2.75, || false, tx), "then still");
        book.select(1);
        assert!(
            !book.leaves.frame(2.8, || true, tx),
            "Reduce Motion turns none"
        );
        assert!(book.leaves.still);
        s.changed();
    });
}

#[test]
fn an_initial_grows_in_once_its_page_has_settled() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "open grimoire");
    press(&mut rt, Key::Enter);
    let first = rt.state.rows()[0].id.clone();
    let second = rt.state.rows()[1].id.clone();
    let grown =
        |rt: &rune::Runtime<Cantrip>, id: &str| rt.state.book.as_ref().and_then(|b| b.grown_at(id));
    // Under the cover, nothing grows.
    rt.update(|s, tx| {
        s.animate_book(10.0, tx);
        s.animate_book(10.3, tx);
    });
    settle(&mut rt);
    assert_eq!(
        grown(&rt, &first),
        None,
        "not while the cover is in the air"
    );
    // Landed: its initial starts growing once the ink has seeped in, and frames keep coming
    // until it has.
    rt.update(|s, tx| {
        s.animate_book(10.7, tx);
        s.animate_book(10.75, tx);
    });
    settle(&mut rt);
    let began = grown(&rt, &first).expect("growing");
    assert!(began > 10.7, "a moment after it settles");
    rt.update(|s, tx| assert!(s.animate_book(began + 0.5, tx), "still growing"));
    settle(&mut rt);
    rt.update(|s, tx| {
        s.animate_book(began + 2.0, tx);
        assert!(!s.animate_book(began + 2.1, tx), "grown, and still");
    });
    // The next spell's grows once the leaf has landed on it, not while it's in the air.
    press(&mut rt, Key::Down);
    rt.update(|s, tx| {
        s.animate_book(20.0, tx);
        s.animate_book(20.2, tx);
    });
    settle(&mut rt);
    assert_eq!(grown(&rt, &second), None);
    rt.update(|s, tx| {
        s.animate_book(20.5, tx);
        s.animate_book(20.55, tx);
    });
    settle(&mut rt);
    assert!(grown(&rt, &second).is_some());
}

#[test]
fn a_turning_leaf_is_captured_with_its_writing_for_the_compositor() {
    let mut rt = runtime(fixtures::root);
    type_text(&mut rt, "open grimoire");
    press(&mut rt, Key::Enter);
    // The book's leaves turn on the frame clock: start one, and catch it mid-air.
    press(&mut rt, Key::Down);
    rt.update(|s, tx| {
        s.animate_book(10.0, tx);
        s.animate_book(10.05, tx);
    });
    settle(&mut rt);
    assert!(
        rt.find("turning").is_some(),
        "lying flat under the capture slot"
    );
    assert!(rt.find("page-capture").is_some());
    assert!(
        rt.find("leaf-source-spell:uuid.ts:uuid").is_some(),
        "its face: the incantation of the spread it closes"
    );
    // Past the spine, its back is up.
    rt.update(|s, tx| {
        s.animate_book(10.35, tx);
    });
    settle(&mut rt);
    assert!(rt.find("page-capture-left").is_some());
    assert!(
        rt.find("leaf-entry-spell:git.ts:branches").is_some(),
        "its back: the entry of the spread it opens"
    );
    rt.update(|s, tx| {
        s.animate_book(11.0, tx);
        s.animate_book(11.05, tx);
    });
    settle(&mut rt);
    assert!(rt.find("turning").is_none(), "gone once it lands");
}

#[test]
fn an_incantation_is_copied_out_from_its_export() {
    use crate::views::book::excerpt;
    let file = "import x from \"y\"\n\nconst helper = 1\n\n// Says hello.\n/** Twice. */\nexport const hello = Spell.make({\n\ttitle: \"Hi\"\n})\n\nexport default Spell.make({})\n";
    assert_eq!(
        excerpt("greet:hello", file)
            .lines()
            .take(4)
            .collect::<Vec<_>>(),
        [
            "// Says hello.",
            "/** Twice. */",
            "export const hello = Spell.make({",
            "  title: \"Hi\""
        ],
        "from the comments over it, tabs as spaces"
    );
    assert_eq!(
        excerpt("greet", file).lines().next(),
        Some("export default Spell.make({})")
    );
    assert_eq!(
        excerpt("greet:missing", file).lines().next(),
        Some("import x from \"y\""),
        "the whole file when the export can't be found"
    );
}
