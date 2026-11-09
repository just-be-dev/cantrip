//! Menu bar items: Cantrip's own, and one per daemon. The host describes a daemon's item
//! (icon, label, menu) and this draws it natively, sending choices back through `on_event`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBitmapImageRep, NSCellImagePosition, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSDeviceRGBColorSpace, NSEventModifierFlags, NSFont, NSFontWeightSemibold, NSImage, NSMenu,
    NSMenuDelegate, NSMenuItem, NSProgressIndicator, NSProgressIndicatorStyle, NSStatusBar,
    NSStatusBarButton, NSStatusItem, NSTextField, NSVariableStatusItemLength, NSView, NSWorkspace,
};
use objc2_foundation::{
    NSDictionary, NSNumber, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes,
    NSSize, NSString, NSTimer,
};
use objc2_quartz_core::{
    CABasicAnimation, CAMediaTiming, CAMediaTimingFunction, kCAMediaTimingFunctionEaseInEaseOut,
};

use super::ring::{self, RingFrame};
use crate::icons::Icon;
use crate::protocol::{WireBarIcon, WireMenuEntry, WireMenuItem, WireStatusItem, WireTone};

// MARK: The target every menu item and menu reports to

struct TargetIvars {
    on_choose: Box<dyn Fn(isize)>,
    on_open: Box<dyn Fn()>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and MenuTarget doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(choose:))]
        fn choose(&self, sender: &NSMenuItem) {
            (self.ivars().on_choose)(sender.tag());
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}

    unsafe impl NSMenuDelegate for MenuTarget {
        #[unsafe(method(menuWillOpen:))]
        fn menu_will_open(&self, _menu: &NSMenu) {
            (self.ivars().on_open)();
        }
    }
);

impl MenuTarget {
    fn new(
        mtm: MainThreadMarker,
        on_choose: impl Fn(isize) + 'static,
        on_open: impl Fn() + 'static,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars {
            on_choose: Box::new(on_choose),
            on_open: Box::new(on_open),
        });
        unsafe { msg_send![super(this), init] }
    }
}

fn choose_sel() -> Sel {
    sel!(choose:)
}

fn menu_item(mtm: MainThreadMarker, title: &str, key: &str) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            None,
            &NSString::from_str(key),
        )
    }
}

// MARK: Cantrip's own item

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainCommand {
    OpenPanel,
    OpenGrimoire,
    OpenJournal,
    ReloadGrimoire,
    RestartHost,
    ToggleLogin,
    Quit,
}

const MAIN_COMMANDS: [MainCommand; 7] = [
    MainCommand::OpenPanel,
    MainCommand::OpenGrimoire,
    MainCommand::OpenJournal,
    MainCommand::ReloadGrimoire,
    MainCommand::RestartHost,
    MainCommand::ToggleLogin,
    MainCommand::Quit,
];

pub struct MainItem {
    item: Retained<NSStatusItem>,
    login: Retained<NSMenuItem>,
    _target: Retained<MenuTarget>,
}

pub struct MainMenuSpec {
    pub shortcut: Option<(String, crate::config::MenuMods)>,
    /// (command, title, tooltip, SF Symbol, key equivalent)
    pub builtins: Vec<(
        MainCommand,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
    )>,
}

impl MainItem {
    pub fn new(
        mtm: MainThreadMarker,
        spec: MainMenuSpec,
        on_command: impl Fn(MainCommand) + 'static,
    ) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(-2.0); // NSSquareStatusItemLength
        if let Some(button) = item.button(mtm) {
            button.setImage(super::symbol_image("wand.and.sparkles", 14.0).as_deref());
            if let Some(image) = button.image() {
                image.setTemplate(true);
            }
        }
        let target = MenuTarget::new(
            mtm,
            move |tag| {
                if let Some(c) = MAIN_COMMANDS.get(tag as usize) {
                    on_command(*c)
                }
            },
            || {},
        );
        let menu = NSMenu::new(mtm);
        let add =
            |command: MainCommand, title: &str, key: &str, icon: &str| -> Retained<NSMenuItem> {
                let row = menu_item(mtm, title, key);
                unsafe {
                    row.setTarget(Some(&target));
                    row.setAction(Some(choose_sel()));
                }
                row.setTag(
                    MAIN_COMMANDS
                        .iter()
                        .position(|c| *c == command)
                        .unwrap_or(0) as isize,
                );
                row.setImage(super::symbol_image(icon, 13.0).as_deref());
                menu.addItem(&row);
                row
            };
        let open = add(
            MainCommand::OpenPanel,
            "Open Cantrip",
            "",
            "wand.and.sparkles",
        );
        // Shown right-aligned in menu glyphs (⌥Space); the global hotkey does the real work.
        if let Some((key, mods)) = &spec.shortcut {
            open.setKeyEquivalent(&NSString::from_str(key));
            let mut mask = NSEventModifierFlags::empty();
            if mods.cmd {
                mask |= NSEventModifierFlags::Command
            }
            if mods.option {
                mask |= NSEventModifierFlags::Option
            }
            if mods.ctrl {
                mask |= NSEventModifierFlags::Control
            }
            if mods.shift {
                mask |= NSEventModifierFlags::Shift
            }
            open.setKeyEquivalentModifierMask(mask);
        }
        let mut group = None;
        for (command, title, tooltip, icon, key) in &spec.builtins {
            let g = matches!(
                command,
                MainCommand::ReloadGrimoire | MainCommand::RestartHost
            );
            if group != Some(g) {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
                group = Some(g);
            }
            add(*command, title, key, icon).setToolTip(Some(&NSString::from_str(tooltip)));
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let login = add(MainCommand::ToggleLogin, "Launch at Login", "", "sunrise");
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        add(MainCommand::Quit, "Quit Cantrip", "q", "power");
        item.setMenu(Some(&menu));
        let main = MainItem {
            item,
            login,
            _target: target,
        };
        main.refresh_login();
        main
    }

    pub fn refresh_login(&self) {
        self.login.setState(if super::login::enabled() {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
}

impl Drop for MainItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

// MARK: Daemon items

pub enum DaemonEvent {
    Chose(Box<WireMenuItem>),
    Opened,
}

/// Every daemon's menu bar item, by daemon id.
pub struct DaemonBar {
    items: HashMap<String, DaemonItem>,
    on_event: OnDaemonEvent,
}

type OnDaemonEvent = Rc<dyn Fn(&str, DaemonEvent)>;

impl DaemonBar {
    pub fn new(on_event: impl Fn(&str, DaemonEvent) + 'static) -> Self {
        DaemonBar {
            items: HashMap::new(),
            on_event: Rc::new(on_event),
        }
    }

    pub fn update(&mut self, id: &str, status: &WireStatusItem) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let on_event = self.on_event.clone();
        self.items
            .entry(id.to_string())
            .or_insert_with(|| DaemonItem::new(mtm, id, on_event))
            .apply(mtm, status);
    }

    pub fn remove(&mut self, id: &str) {
        self.items.remove(id);
    }

    pub fn remove_all(&mut self) {
        self.items.clear();
    }
}

struct DaemonItem {
    item: Retained<NSStatusItem>,
    menu: Retained<NSMenu>,
    /// The choosable items, by menu item tag.
    choices: Rc<RefCell<Vec<WireMenuItem>>>,
    current: Option<WireStatusItem>,
    ring: Option<Rc<RingAnimator>>,
    _target: Retained<MenuTarget>,
}

impl DaemonItem {
    fn new(mtm: MainThreadMarker, id: &str, on_event: OnDaemonEvent) -> Self {
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        // Remembers where the user dragged it (⌘-drag) across launches.
        item.setAutosaveName(Some(&NSString::from_str(&format!("cantrip.daemon.{id}"))));
        let choices: Rc<RefCell<Vec<WireMenuItem>>> = Rc::default();
        let (on_choose, on_open, chosen) = (on_event.clone(), on_event, choices.clone());
        let (id_choose, id_open) = (id.to_string(), id.to_string());
        let target = MenuTarget::new(
            mtm,
            move |tag| {
                let item = chosen.borrow().get(tag as usize).cloned();
                if let Some(item) = item {
                    on_choose(&id_choose, DaemonEvent::Chose(Box::new(item)))
                }
            },
            move || on_open(&id_open, DaemonEvent::Opened),
        );
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        item.setMenu(Some(&menu));
        DaemonItem {
            item,
            menu,
            choices,
            current: None,
            ring: None,
            _target: target,
        }
    }

    fn apply(&mut self, mtm: MainThreadMarker, status: &WireStatusItem) {
        let previous = self.current.replace(status.clone());
        let Some(button) = self.item.button(mtm) else {
            return;
        };
        if previous
            .as_ref()
            .is_none_or(|p| p.icon != status.icon || p.label != status.label)
        {
            self.render_button(&button, status);
        }
        let tip = NSString::from_str(status.tooltip.as_deref().unwrap_or(&status.title));
        button.setToolTip(Some(&tip));
        unsafe {
            let _: () = msg_send![&*button, setAccessibilityLabel: &*tip];
        }
        // Rebuilding an open menu closes its submenus, so only do it when something changed.
        if previous.as_ref().is_none_or(|p| p.menu != status.menu) {
            self.menu.removeAllItems();
            let mut choices = Vec::new();
            for entry in &status.menu {
                self.menu.addItem(&self.entry(mtm, entry, &mut choices));
            }
            *self.choices.borrow_mut() = choices;
        }
    }

    fn render_button(&mut self, button: &NSStatusBarButton, status: &WireStatusItem) {
        let label = match status.label.as_deref() {
            Some("") | None => String::new(),
            Some(l) => format!(" {l}"),
        };
        match &status.icon {
            WireBarIcon::Ring {
                value,
                tone,
                busy,
                pulse,
                badge,
            } => {
                let ring = self.ring.get_or_insert_with(|| RingAnimator::new(button));
                ring.update(*value, *tone, *busy, *pulse, *badge);
                button.setTitle(&NSString::from_str(&label));
            }
            WireBarIcon::Image { value } => {
                if let Some(ring) = self.ring.take() {
                    ring.stop();
                }
                match Icon::parse(Some(value)) {
                    Icon::Emoji(text) => {
                        button.setImage(None);
                        button.setTitle(&NSString::from_str(&format!("{text}{label}")));
                    }
                    Icon::Symbol(name) => {
                        let image = super::symbol_image(&name, 14.0)
                            .or_else(|| super::symbol_image("sparkle", 14.0));
                        if let Some(image) = &image {
                            image.setTemplate(true);
                        }
                        button.setImage(image.as_deref());
                        button.setTitle(&NSString::from_str(&label));
                    }
                    Icon::File(path) => {
                        let image =
                            NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path));
                        image.setSize(NSSize::new(18.0, 18.0));
                        button.setImage(Some(&image));
                        button.setTitle(&NSString::from_str(&label));
                    }
                }
            }
        }
        button.setImagePosition(if label.is_empty() && button.image().is_some() {
            NSCellImagePosition::ImageOnly
        } else {
            NSCellImagePosition::ImageLeading
        });
    }

    fn entry(
        &self,
        mtm: MainThreadMarker,
        entry: &WireMenuEntry,
        choices: &mut Vec<WireMenuItem>,
    ) -> Retained<NSMenuItem> {
        match entry {
            WireMenuEntry::Separator => NSMenuItem::separatorItem(mtm),
            WireMenuEntry::Header { title } => {
                NSMenuItem::sectionHeaderWithTitle(&NSString::from_str(title), mtm)
            }
            WireMenuEntry::Item(item) => {
                if let Some(progress) = item.progress {
                    let row = NSMenuItem::new(mtm);
                    row.setView(Some(&gauge_row(mtm, item, progress)));
                    return row;
                }
                let row = menu_item(mtm, &item.title, item.key.as_deref().unwrap_or(""));
                if let Some(subtitle) = &item.subtitle {
                    row.setSubtitle(Some(&NSString::from_str(subtitle)));
                }
                row.setImage(menu_image(item.icon.as_deref(), item.tone).as_deref());
                row.setState(if item.checked {
                    NSControlStateValueOn
                } else {
                    NSControlStateValueOff
                });
                row.setEnabled(item.enabled);
                if item.action != crate::protocol::MenuAction::None {
                    unsafe {
                        row.setTarget(Some(&self._target));
                        row.setAction(Some(choose_sel()));
                    }
                    row.setTag(choices.len() as isize);
                    choices.push(item.clone());
                }
                if !item.submenu.is_empty() {
                    let submenu = NSMenu::new(mtm);
                    submenu.setAutoenablesItems(false);
                    for child in &item.submenu {
                        submenu.addItem(&self.entry(mtm, child, choices));
                    }
                    row.setSubmenu(Some(&submenu));
                }
                row
            }
        }
    }
}

impl Drop for DaemonItem {
    fn drop(&mut self) {
        if let Some(ring) = self.ring.take() {
            ring.stop();
        }
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

fn tone_color(tone: WireTone) -> Option<Retained<NSColor>> {
    match tone {
        WireTone::Normal => None,
        WireTone::Warning => Some(NSColor::systemOrangeColor()),
        WireTone::Critical => Some(NSColor::systemRedColor()),
    }
}

fn tone_rgba(tone: WireTone) -> Option<[f32; 4]> {
    match tone {
        WireTone::Normal => None,
        WireTone::Warning => Some([1.0, 0.584, 0.0, 1.0]),
        WireTone::Critical => Some([1.0, 0.231, 0.188, 1.0]),
    }
}

fn menu_image(raw: Option<&str>, tone: WireTone) -> Option<Retained<NSImage>> {
    match Icon::parse(Some(raw?)) {
        Icon::Symbol(name) => match tone_rgba(tone) {
            None => super::symbol_image(&name, 13.0),
            Some(rgba) => {
                let (w, h, px) = super::symbol_rgba(&name, 13.0, 16.0, 2.0, rgba)?;
                image_from_rgba(w, h, &px, 16.0, 16.0)
            }
        },
        Icon::Emoji(text) => {
            let font = NSFont::systemFontOfSize(13.0);
            let string = NSString::from_str(&text);
            let key = NSString::from_str("NSFont");
            let attrs = NSDictionary::from_retained_objects(
                &[&*key],
                &[Retained::into_super(Retained::into_super(font)) as Retained<AnyObject>],
            );
            let size: NSSize = unsafe { msg_send![&*string, sizeWithAttributes: &*attrs] };
            let (w, h, px) = super::rasterize(
                size.width.ceil(),
                size.height.ceil(),
                2.0,
                |_| unsafe {
                    let _: () = msg_send![&*string, drawAtPoint: NSPoint::new(0.0, 0.0), withAttributes: &*attrs];
                },
            )?;
            image_from_rgba(w, h, &px, size.width.ceil(), size.height.ceil())
        }
        Icon::File(path) => {
            let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path));
            image.setSize(NSSize::new(16.0, 16.0));
            Some(image)
        }
    }
}

/// An NSImage `w_pt`×`h_pt` points from straight-alpha RGBA pixels.
fn image_from_rgba(w: u32, h: u32, rgba: &[u8], w_pt: f64, h_pt: f64) -> Option<Retained<NSImage>> {
    use objc2::AnyThread;
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(), std::ptr::null_mut(), w as isize, h as isize, 8, 4, true, false, NSDeviceRGBColorSpace, w as isize * 4, 32,
        )?
    };
    let data = rep.bitmapData();
    if data.is_null() {
        return None;
    }
    let out = unsafe { std::slice::from_raw_parts_mut(data, rgba.len()) };
    for (o, p) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(rgba.as_chunks::<4>().0)
    {
        let a = p[3] as u32;
        let pre = |c: u8| ((c as u32 * a + 127) / 255) as u8;
        o.copy_from_slice(&[pre(p[0]), pre(p[1]), pre(p[2]), p[3]]);
    }
    rep.setSize(NSSize::new(w_pt, h_pt));
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(w_pt, h_pt));
    image.addRepresentation(&rep);
    Some(image)
}

/// A menu row with a bar: title and accessory on top, the bar, then the subtitle.
fn gauge_row(mtm: MainThreadMarker, item: &WireMenuItem, progress: f64) -> Retained<NSView> {
    const WIDTH: f64 = 290.0;
    let (lead, trail, pad) = (16.0, 14.0, 5.0);
    let subtitle_h = if item.subtitle.is_some() { 18.0 } else { 0.0 };
    let height = pad + 17.0 + 4.0 + 8.0 + subtitle_h + pad;
    let view = NSView::initWithFrame(
        NSView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(WIDTH, height)),
    );
    let top = height - pad - 17.0;

    let title = NSTextField::labelWithString(&NSString::from_str(&item.title), mtm);
    title.setFont(Some(&NSFont::systemFontOfSize(13.0)));
    title.setFrame(NSRect::new(
        NSPoint::new(lead, top),
        NSSize::new(WIDTH - lead - trail - 80.0, 17.0),
    ));
    view.addSubview(&title);

    if let Some(accessory) = &item.accessory {
        let label = NSTextField::labelWithString(&NSString::from_str(accessory), mtm);
        label.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(
            13.0,
            unsafe { NSFontWeightSemibold },
        )));
        if let Some(color) = tone_color(item.tone) {
            label.setTextColor(Some(&color));
        }
        label.setAlignment(objc2_app_kit::NSTextAlignment::Right);
        label.setFrame(NSRect::new(
            NSPoint::new(WIDTH - trail - 120.0, top),
            NSSize::new(120.0, 17.0),
        ));
        view.addSubview(&label);
    }

    let bar = NSProgressIndicator::initWithFrame(
        NSProgressIndicator::alloc(mtm),
        NSRect::new(
            NSPoint::new(lead, top - 4.0 - 8.0),
            NSSize::new(WIDTH - lead - trail, 8.0),
        ),
    );
    bar.setStyle(NSProgressIndicatorStyle::Bar);
    bar.setIndeterminate(false);
    bar.setMinValue(0.0);
    bar.setMaxValue(1.0);
    bar.setDoubleValue(progress.clamp(0.0, 1.0));
    view.addSubview(&bar);

    if let Some(subtitle) = &item.subtitle {
        let label = NSTextField::labelWithString(&NSString::from_str(subtitle), mtm);
        label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFrame(NSRect::new(
            NSPoint::new(lead, pad),
            NSSize::new(WIDTH - lead - trail, 15.0),
        ));
        view.addSubview(&label);
    }
    view
}

// MARK: The ring

/// Drives a status item button's ring at display rate while something moves: eases between
/// values (filling up from empty on the first one), spins while busy, and pulses the button's
/// layer when asked. The timer stops at rest.
struct RingAnimator {
    button: Retained<NSStatusBarButton>,
    state: RefCell<RingState>,
    timer: RefCell<Option<Retained<NSTimer>>>,
    pulsing: Cell<bool>,
}

#[derive(Default)]
struct RingState {
    shown: Option<f64>,
    from: f64,
    target: Option<f64>,
    has_target: bool,
    sweep_start: Option<Instant>,
    busy_start: Option<Instant>,
    badge: bool,
    tone: WireTone,
    pulse: bool,
}

const SWEEP: f64 = 1.1;

impl RingAnimator {
    fn new(button: &NSStatusBarButton) -> Rc<Self> {
        Rc::new(RingAnimator {
            button: button.retain(),
            state: RefCell::default(),
            timer: RefCell::new(None),
            pulsing: Cell::new(false),
        })
    }

    fn update(
        self: &Rc<Self>,
        value: Option<f64>,
        tone: WireTone,
        busy: bool,
        pulse: bool,
        badge: bool,
    ) {
        let now = Instant::now();
        {
            let mut s = self.state.borrow_mut();
            if !s.has_target || value != s.target {
                s.from = s.shown.unwrap_or(0.0);
                s.target = value;
                s.has_target = true;
                s.sweep_start = Some(now);
                if value.is_none() {
                    s.shown = None;
                }
            }
            if busy && s.busy_start.is_none() {
                s.busy_start = Some(now);
            } else if !busy {
                s.busy_start = None;
            }
            (s.badge, s.tone, s.pulse) = (badge, tone, pulse);
        }
        self.render(now);
        if self.needs_animation(now) {
            self.start_timer();
        }
        self.update_pulse();
    }

    fn stop(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.invalidate();
        }
        if let Some(layer) = self.button.layer() {
            layer.removeAnimationForKey(&NSString::from_str("pulse"));
        }
        self.pulsing.set(false);
    }

    fn reduce_motion() -> bool {
        super::reduce_motion()
    }

    /// A Core Animation opacity loop: it runs in the render server, so it's cheap even for hours.
    fn update_pulse(&self) {
        let should = self.state.borrow().pulse && !Self::reduce_motion();
        if should == self.pulsing.get() {
            return;
        }
        self.button.setWantsLayer(true);
        let Some(layer) = self.button.layer() else {
            return;
        };
        let key = NSString::from_str("pulse");
        if should {
            let anim = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
            unsafe {
                anim.setFromValue(Some(&NSNumber::new_f64(1.0)));
                anim.setToValue(Some(&NSNumber::new_f64(0.35)));
            }
            anim.setDuration(0.8);
            anim.setAutoreverses(true);
            anim.setRepeatCount(f32::INFINITY);
            anim.setTimingFunction(Some(&CAMediaTimingFunction::functionWithName(unsafe {
                kCAMediaTimingFunctionEaseInEaseOut
            })));
            layer.addAnimation_forKey(&anim, Some(&key));
        } else {
            layer.removeAnimationForKey(&key);
        }
        self.pulsing.set(should);
    }

    fn needs_animation(&self, now: Instant) -> bool {
        if Self::reduce_motion() {
            return false;
        }
        let s = self.state.borrow();
        let sweeping = s.target.is_some()
            && s.sweep_start
                .is_some_and(|t| now.duration_since(t).as_secs_f64() < SWEEP);
        sweeping || s.busy_start.is_some()
    }

    fn start_timer(self: &Rc<Self>) {
        if self.timer.borrow().is_some() {
            return;
        }
        let me = Rc::downgrade(self);
        let block = RcBlock::new(move |_: std::ptr::NonNull<NSTimer>| {
            if let Some(me) = me.upgrade() {
                me.tick();
            }
        });
        let timer =
            unsafe { NSTimer::timerWithTimeInterval_repeats_block(1.0 / 60.0, true, &block) };
        // Common modes, so it keeps animating while the menu is open.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        *self.timer.borrow_mut() = Some(timer);
    }

    fn tick(&self) {
        let now = Instant::now();
        self.render(now);
        if !self.needs_animation(now) {
            if let Some(timer) = self.timer.borrow_mut().take() {
                timer.invalidate();
            }
            self.render(now);
        }
    }

    fn render(&self, now: Instant) {
        let frame = {
            let mut s = self.state.borrow_mut();
            if let Some(target) = s.target {
                s.shown = Some(if Self::reduce_motion() {
                    target
                } else {
                    let t = s.sweep_start.map_or(1.0, |at| {
                        (now.duration_since(at).as_secs_f64() / SWEEP).clamp(0.0, 1.0)
                    });
                    s.from + (target - s.from) * (1.0 - (1.0 - t).powi(3))
                });
            }
            let spinner = s
                .busy_start
                .filter(|_| !Self::reduce_motion())
                .map(|at| now.duration_since(at).as_secs_f64() * std::f64::consts::TAU / 0.9);
            RingFrame {
                value: s.shown,
                spinner,
                badge: s.badge,
                tone: s.tone,
            }
        };
        let (w, h, px) = ring::rgba(&frame, 2.0);
        if let Some(image) = image_from_rgba(w, h, &px, ring::SIZE, ring::SIZE) {
            image.setTemplate(frame.tone == WireTone::Normal);
            self.button.setImage(Some(&image));
        }
    }
}

impl Drop for RingAnimator {
    fn drop(&mut self) {
        self.stop();
    }
}
