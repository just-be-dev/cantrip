// Menu bar items owned by daemons. The host describes each item (icon, label,
// menu) and this draws it natively, sending menu choices back.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

@MainActor
final class DaemonBar {
    private var items: [String: DaemonStatusItem] = [:]
    private let host: SpellHost
    /// Copy / open / HUD, shared with the bar.
    private let perform: (DaemonStatusItem.Choice) -> Void

    init(host: SpellHost, perform: @escaping (DaemonStatusItem.Choice) -> Void) {
        self.host = host
        self.perform = perform
    }

    func update(_ id: String, _ status: WireStatusItem) {
        if let existing = items[id] {
            existing.apply(status)
        } else {
            let item = DaemonStatusItem(id: id, host: host, perform: perform)
            item.apply(status)
            items[id] = item
        }
    }

    func remove(_ id: String) {
        items.removeValue(forKey: id)?.dispose()
    }

    func removeAll() {
        for item in items.values { item.dispose() }
        items.removeAll()
    }
}

@MainActor
final class DaemonStatusItem: NSObject, NSMenuDelegate {
    enum Choice {
        case copy(String)
        case open(String)
    }

    private let id: String
    private let host: SpellHost
    private let perform: (Choice) -> Void
    private let statusItem: NSStatusItem
    private let menu = NSMenu()
    private var animator: RingAnimator?
    private var current: WireStatusItem?

    init(id: String, host: SpellHost, perform: @escaping (Choice) -> Void) {
        self.id = id
        self.host = host
        self.perform = perform
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        // Remembers where the user dragged it (⌘-drag) across launches.
        statusItem.autosaveName = "cantrip.daemon.\(id)"
        menu.autoenablesItems = false
        menu.delegate = self
        statusItem.menu = menu
    }

    func dispose() {
        animator?.stop()
        NSStatusBar.system.removeStatusItem(statusItem)
    }

    func apply(_ status: WireStatusItem) {
        let previous = current
        current = status
        guard let button = statusItem.button else { return }

        if previous?.icon != status.icon || previous?.label != status.label {
            renderButton(button, status)
        }
        button.toolTip = status.tooltip ?? status.title
        button.setAccessibilityLabel(status.tooltip ?? status.title)
        // Rebuilding an open menu closes its submenus, so only do it when something changed.
        if previous?.menu != status.menu {
            menu.removeAllItems()
            for entry in status.menu { menu.addItem(menuItem(entry)) }
        }
    }

    private func renderButton(_ button: NSStatusBarButton, _ status: WireStatusItem) {
        let label = status.label.map { $0.isEmpty ? "" : " \($0)" } ?? ""
        switch status.icon {
        case let .ring(value, tone, busy, pulse, badge):
            if animator == nil { animator = RingAnimator(button: button) }
            animator?.update(value: value, tone: tone, busy: busy, pulse: pulse, badge: badge)
            button.title = label
        case let .image(raw):
            animator?.stop()
            animator = nil
            switch Icon(raw) {
            case let .emoji(text):
                button.image = nil
                button.title = text + label
            case let .symbol(name):
                let image = NSImage(systemSymbolName: name, accessibilityDescription: status.title)
                    ?? NSImage(systemSymbolName: "sparkle", accessibilityDescription: status.title)
                image?.isTemplate = true
                button.image = image
                button.title = label
            case let .file(path):
                let image = NSWorkspace.shared.icon(forFile: path)
                image.size = NSSize(width: 18, height: 18)
                button.image = image
                button.title = label
            case let .contentType(identifier):
                let image = NSWorkspace.shared.icon(for: UTType(identifier) ?? .data)
                image.size = NSSize(width: 18, height: 18)
                button.image = image
                button.title = label
            }
        }
        button.imagePosition = button.title.isEmpty ? .imageOnly : .imageLeading
    }

    // MARK: Menu

    func menuWillOpen(_ menu: NSMenu) {
        host.send(.menuOpened(daemon: id))
    }

    private func menuItem(_ entry: WireMenuEntry) -> NSMenuItem {
        switch entry {
        case .separator:
            return .separator()
        case let .header(title):
            return .sectionHeader(title: title)
        case let .item(item):
            if let progress = item.progress {
                let row = NSMenuItem()
                let view = NSHostingView(rootView: GaugeRow(item: item, progress: progress))
                view.frame.size = NSSize(width: 290, height: view.fittingSize.height)
                row.view = view
                return row
            }
            let row = NSMenuItem(title: item.title, action: nil, keyEquivalent: item.key ?? "")
            if let subtitle = item.subtitle { row.subtitle = subtitle }
            row.image = Self.menuImage(item.icon, tone: item.tone)
            row.state = item.checked ? .on : .off
            row.isEnabled = item.enabled
            if item.action != .none {
                row.target = self
                row.action = #selector(choose(_:))
                row.representedObject = item
            }
            if !item.submenu.isEmpty {
                let submenu = NSMenu()
                submenu.autoenablesItems = false
                for child in item.submenu { submenu.addItem(menuItem(child)) }
                row.submenu = submenu
            }
            return row
        }
    }

    @objc private func choose(_ sender: NSMenuItem) {
        guard let item = sender.representedObject as? WireMenuItem else { return }
        switch item.action {
        case .callback: host.send(.menu(daemon: id, item: item.id))
        case .copy: perform(.copy(item.value ?? ""))
        case .open: perform(.open(item.value ?? ""))
        case .none: break
        }
    }

    static func menuImage(_ raw: String?, tone: WireTone) -> NSImage? {
        guard let raw else { return nil }
        switch Icon(raw) {
        case let .symbol(name):
            guard let image = NSImage(systemSymbolName: name, accessibilityDescription: nil) else { return nil }
            guard let color = tone.color else { return image }
            return image.withSymbolConfiguration(.init(paletteColors: [color]))
        case let .emoji(text):
            let attributed = NSAttributedString(string: text, attributes: [.font: NSFont.systemFont(ofSize: 13)])
            let size = attributed.size()
            return NSImage(size: size, flipped: false) { _ in
                attributed.draw(at: .zero)
                return true
            }
        case let .file(path):
            let image = NSWorkspace.shared.icon(forFile: path)
            image.size = NSSize(width: 16, height: 16)
            return image
        case let .contentType(identifier):
            let image = NSWorkspace.shared.icon(for: UTType(identifier) ?? .data)
            image.size = NSSize(width: 16, height: 16)
            return image
        }
    }
}

extension WireTone {
    var color: NSColor? {
        switch self {
        case .normal: nil
        case .warning: .systemOrange
        case .critical: .systemRed
        }
    }

    var tint: Color {
        switch self {
        case .normal: .accentColor
        case .warning: .orange
        case .critical: .red
        }
    }
}

/// A menu row with a bar: title and accessory on top, the bar, then the subtitle.
private struct GaugeRow: View {
    let item: WireMenuItem
    let progress: Double

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                Text(item.title).font(.system(size: 13))
                Spacer()
                if let accessory = item.accessory {
                    Text(accessory)
                        .font(.system(size: 13).monospacedDigit().weight(.semibold))
                        .foregroundStyle(item.tone == .normal ? Color.primary : item.tone.tint)
                }
            }
            ProgressView(value: progress, total: 1).tint(item.tone.tint)
            if let subtitle = item.subtitle {
                Text(subtitle).font(.system(size: 11)).foregroundStyle(.secondary)
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 14)
        .padding(.vertical, 5)
        .frame(width: 290, alignment: .leading)
    }
}
