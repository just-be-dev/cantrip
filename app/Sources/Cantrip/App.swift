import AppKit
import ServiceManagement

@main
@MainActor
enum CantripMain {
    static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
        _ = delegate
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var config = CantripConfig.load()
    private var host: SpellHost!
    private var store: Store!
    private var panel: PanelController!
    private var hotKey: HotKey?
    private var statusItem: NSStatusItem?

    func applicationDidFinishLaunching(_ notification: Notification) {
        host = SpellHost(config: config)
        store = Store(host: host)
        panel = PanelController(store: store)

        host.onMessage = { [weak self] message in self?.store.receive(message) }
        host.onRestart = { [weak self] in self?.store.hostRestarted() }
        host.start()

        Task { store.apps = await AppCatalog.scan() }
        // Pick up newly installed apps whenever the bar is opened after a while.
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didLaunchApplicationNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                _ = Task { self?.store.apps = await AppCatalog.scan() }
            }
        }

        registerHotKey()
        enableLoginItemOnFirstRun()
        installStatusItem()

        if ProcessInfo.processInfo.environment["CANTRIP_SHOW_ON_LAUNCH"] != nil { panel.show() }
    }

    func applicationWillTerminate(_ notification: Notification) {
        host.stop()
    }

    private func registerHotKey() {
        guard let parsed = config.parsedHotkey else {
            NSLog("Cantrip: can't parse hotkey \"\(config.hotkey)\"")
            return
        }
        hotKey = HotKey(keyCode: parsed.keyCode, modifiers: parsed.modifiers) { [weak self] in
            self?.panel.toggle()
        }
    }

    private func installStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = NSImage(systemSymbolName: "wand.and.sparkles", accessibilityDescription: "Cantrip")
        let menu = NSMenu()
        let open = menu.addItem(withTitle: "Open Cantrip", action: #selector(openPanel), keyEquivalent: "")
        open.target = self
        open.image = NSImage(systemSymbolName: "wand.and.sparkles", accessibilityDescription: nil)
        // Shown right-aligned in menu glyphs (⌘Space); the global hotkey does the real work.
        if let shortcut = config.menuShortcut {
            open.keyEquivalent = shortcut.key
            open.keyEquivalentModifierMask = shortcut.modifiers
        }
        menu.addItem(.separator())
        addItem(menu, .openGrimoire, #selector(openGrimoire))
        addItem(menu, .openJournal, #selector(openJournal))
        menu.addItem(.separator())
        addItem(menu, .shortRest, #selector(shortRest), "r")
        addItem(menu, .longRest, #selector(longRest))
        menu.addItem(.separator())
        let login = menu.addItem(withTitle: "Launch at Login", action: #selector(toggleLoginItem(_:)), keyEquivalent: "")
        login.target = self
        login.image = NSImage(systemSymbolName: "sunrise", accessibilityDescription: nil)
        login.state = SMAppService.mainApp.status == .enabled ? .on : .off
        menu.addItem(.separator())
        let quit = menu.addItem(withTitle: "Quit Cantrip", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        quit.image = NSImage(systemSymbolName: Builtin.quit.icon, accessibilityDescription: nil)
        item.menu = menu
        statusItem = item
    }

    private func addItem(_ menu: NSMenu, _ builtin: Builtin, _ action: Selector, _ key: String = "") {
        let item = menu.addItem(withTitle: builtin.title, action: action, keyEquivalent: key)
        item.target = self
        item.toolTip = builtin.detail
        item.image = NSImage(systemSymbolName: builtin.icon, accessibilityDescription: nil)
    }

    @objc private func openPanel() { panel.show() }
    @objc private func shortRest() { store.perform(builtin: .shortRest) }
    @objc private func longRest() { store.perform(builtin: .longRest) }
    @objc private func openGrimoire() { NSWorkspace.shared.open(CantripConfig.grimoireDirectory) }
    @objc private func openJournal() { NSWorkspace.shared.open(host.logURL) }

    @objc private func toggleLoginItem(_ sender: NSMenuItem) {
        let service = SMAppService.mainApp
        do {
            if service.status == .enabled { try service.unregister() } else { try service.register() }
        } catch {
            NSLog("Cantrip: login item change failed: \(error)")
        }
        sender.state = service.status == .enabled ? .on : .off
    }

    /// A launcher should be there after a reboot, so the installed app turns this on once.
    /// Skipped for `swift run` builds, which aren't in an app bundle.
    private func enableLoginItemOnFirstRun() {
        let key = "didSetUpLoginItem"
        guard Bundle.main.bundleURL.pathExtension == "app", !UserDefaults.standard.bool(forKey: key) else { return }
        UserDefaults.standard.set(true, forKey: key)
        do {
            try SMAppService.mainApp.register()
        } catch {
            NSLog("Cantrip: could not register login item: \(error)")
        }
    }
}
