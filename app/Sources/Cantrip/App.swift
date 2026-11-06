import AppKit

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
        menu.addItem(withTitle: "Open Cantrip (\(config.hotkey))", action: #selector(openPanel), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Reload Spells", action: #selector(reloadSpells), keyEquivalent: "r").target = self
        menu.addItem(withTitle: "Restart Spell Host", action: #selector(restartHost), keyEquivalent: "").target = self
        menu.addItem(withTitle: "Open Spells Folder", action: #selector(openSpells), keyEquivalent: "").target = self
        menu.addItem(withTitle: "Open Host Log", action: #selector(openLog), keyEquivalent: "").target = self
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Cantrip", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        item.menu = menu
        statusItem = item
    }

    @objc private func openPanel() { panel.show() }
    @objc private func reloadSpells() { store.perform(builtin: .reloadSpells) }
    @objc private func restartHost() { host.restart() }
    @objc private func openSpells() { NSWorkspace.shared.open(CantripConfig.spellsDirectory) }
    @objc private func openLog() { NSWorkspace.shared.open(host.logURL) }
}
