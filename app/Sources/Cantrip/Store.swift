// App state: the root search, the running spell session, and what to do on each key.

import AppKit
import Observation

/// One row in a list, from the root search or from a spell.
struct Row: Identifiable {
    let id: String
    let title: String
    let subtitle: String?
    let icon: Icon
    let accessories: [String]
    let actions: [ActionItem]
}

struct ActionItem: Identifiable {
    enum Kind {
        case wire(WireAction)
        case runSpell(WireCommand)
        case launchApp(AppEntry)
        case revealInFinder(String)
        case copyText(String)
        case builtin(Builtin)
    }
    let id: String
    let title: String
    let icon: String?
    let kind: Kind
}

enum Builtin: String, CaseIterable {
    case reloadSpells, openGrimoire, openLog, quit

    var title: String {
        switch self {
        case .reloadSpells: "Reload Spells"
        case .openGrimoire: "Open Grimoire"
        case .openLog: "Open Spell Host Log"
        case .quit: "Quit Cantrip"
        }
    }

    var icon: String {
        switch self {
        case .reloadSpells: "arrow.clockwise"
        case .openGrimoire: "books.vertical"
        case .openLog: "doc.text.magnifyingglass"
        case .quit: "power"
        }
    }
}

struct Toast: Equatable {
    let style: ToastStyle
    let title: String
    let message: String?
}

/// A view in a spell's stack. `view == nil` means the spell hasn't rendered it yet.
struct Frame {
    var view: WireView?
    var query = ""
    var selection = 0
}

@Observable
@MainActor
final class Session {
    let id: String
    let command: WireCommand
    var frames: [Frame] = [Frame()]
    var hostLoading = false
    var awaitingPush = false
    var error: (title: String, message: String)?

    init(id: String, command: WireCommand) {
        self.id = id
        self.command = command
    }

    var loading: Bool { hostLoading || awaitingPush || frames.last?.view == nil && error == nil }
}

@Observable
@MainActor
final class Store {
    var commands: [WireCommand] = []
    var loadErrors: [String] = []
    var apps: [AppEntry] = []
    var session: Session?
    var actionsOpen = false
    var actionsSelection = 0
    var toast: Toast?
    /// Bumped to ask the search field to take focus.
    var focusToken = 0

    private var rootQuery = ""
    private var rootSelection = 0
    private var nextSession = 0
    private var toastTask: Task<Void, Never>?
    private var usage: [String: Int] = UserDefaults.standard.dictionary(forKey: "usage") as? [String: Int] ?? [:]

    @ObservationIgnored let host: SpellHost
    @ObservationIgnored var hidePanel: () -> Void = {}
    @ObservationIgnored var showHUD: (String) -> Void = { _ in }

    init(host: SpellHost) {
        self.host = host
    }

    // MARK: Query & selection

    var query: String {
        get { session?.frames.last?.query ?? rootQuery }
        set {
            guard newValue != query else { return }
            actionsOpen = false
            if let session, let depth = session.frames.indices.last {
                session.frames[depth].query = newValue
                session.frames[depth].selection = 0
                if case let .list(_, filtering, _, _)? = session.frames[depth].view, !filtering {
                    host.send(.query(session: session.id, depth: depth, text: newValue))
                }
            } else {
                rootQuery = newValue
                rootSelection = 0
            }
        }
    }

    var selection: Int {
        get { session?.frames.last?.selection ?? rootSelection }
        set {
            if let session, let depth = session.frames.indices.last {
                session.frames[depth].selection = newValue
            } else {
                rootSelection = newValue
            }
        }
    }

    var placeholder: String {
        guard let session else { return "Cast a spell or open an app…" }
        if case let .list(placeholder, _, _, _)? = session.frames.last?.view, let placeholder { return placeholder }
        return "Search \(session.command.title)…"
    }

    // MARK: What's on screen

    var currentView: WireView? { session?.frames.last?.view }

    var rows: [Row] {
        guard let session else { return rootRows }
        guard case let .list(_, filtering, _, items)? = session.frames.last?.view else { return [] }
        let rows = items.map { item in
            Row(
                id: item.id, title: item.title, subtitle: item.subtitle, icon: Icon(item.icon),
                accessories: item.accessories,
                actions: item.actions.map { ActionItem(id: $0.id, title: $0.title, icon: $0.icon, kind: .wire($0)) })
        }
        guard filtering, !query.isEmpty else { return rows }
        return zip(rows, items)
            .compactMap { row, item -> (Row, Int)? in
                guard let score = Fuzzy.score(query, fields: [item.title, item.subtitle ?? ""] + item.keywords) else { return nil }
                return (row, score)
            }
            .sorted { $0.1 > $1.1 }
            .map(\.0)
    }

    var selectedRow: Row? {
        let rows = rows
        return rows.indices.contains(selection) ? rows[selection] : nil
    }

    var currentActions: [ActionItem] {
        if case let .detail(_, _, actions)? = currentView {
            return actions.map { ActionItem(id: $0.id, title: $0.title, icon: $0.icon, kind: .wire($0)) }
        }
        return selectedRow?.actions ?? []
    }

    private var rootRows: [Row] {
        var candidates: [(row: Row, fields: [String], usage: Int)] = []
        for command in commands {
            let row = Row(
                id: "spell:\(command.id)", title: command.title, subtitle: command.subtitle,
                icon: Icon(command.icon ?? "sparkles"), accessories: ["Spell"],
                actions: [
                    ActionItem(id: "run", title: "Cast Spell", icon: "sparkles", kind: .runSpell(command)),
                    ActionItem(id: "copy-id", title: "Copy Spell ID", icon: "doc.on.doc", kind: .copyText(command.id)),
                ])
            candidates.append((row, [command.title, command.subtitle ?? ""] + command.keywords, usage[row.id] ?? 0))
        }
        for app in apps {
            let row = Row(
                id: "app:\(app.path)", title: app.name, subtitle: nil, icon: .file(app.path),
                accessories: ["Application"],
                actions: [
                    ActionItem(id: "open", title: "Open Application", icon: "arrow.up.forward.app", kind: .launchApp(app)),
                    ActionItem(id: "reveal", title: "Show in Finder", icon: "folder", kind: .revealInFinder(app.path)),
                    ActionItem(id: "copy-path", title: "Copy Path", icon: "doc.on.doc", kind: .copyText(app.path)),
                ])
            candidates.append((row, [app.name], usage[row.id] ?? 0))
        }
        for builtin in Builtin.allCases {
            let row = Row(
                id: "builtin:\(builtin.rawValue)", title: builtin.title, subtitle: nil, icon: Icon(builtin.icon),
                accessories: ["Cantrip"],
                actions: [ActionItem(id: "run", title: builtin.title, icon: builtin.icon, kind: .builtin(builtin))])
            candidates.append((row, [builtin.title], usage[row.id] ?? 0))
        }

        if rootQuery.isEmpty {
            // Most used first; spells before apps when tied.
            return candidates.enumerated()
                .sorted { ($0.element.usage, -$0.offset) > ($1.element.usage, -$1.offset) }
                .map(\.element.row)
        }
        return candidates
            .compactMap { candidate -> (Row, Int)? in
                guard let score = Fuzzy.score(rootQuery, fields: candidate.fields) else { return nil }
                return (candidate.row, score + min(candidate.usage * 15, 300))
            }
            .sorted { $0.1 > $1.1 }
            .map(\.0)
    }

    // MARK: Actions

    func perform(_ action: ActionItem) {
        actionsOpen = false
        switch action.kind {
        case let .runSpell(command):
            bumpUsage("spell:\(command.id)")
            run(command)
        case let .launchApp(app):
            bumpUsage("app:\(app.path)")
            NSWorkspace.shared.openApplication(at: URL(filePath: app.path), configuration: .init())
            hidePanel()
        case let .revealInFinder(path):
            NSWorkspace.shared.activateFileViewerSelecting([URL(filePath: path)])
            hidePanel()
        case let .copyText(text):
            copy(text)
        case let .builtin(builtin):
            perform(builtin: builtin)
        case let .wire(wire):
            perform(wire: wire)
        }
    }

    private func perform(wire action: WireAction) {
        guard let session, let depth = session.frames.indices.last else { return }
        switch action.kind {
        case .copy:
            copy(action.value ?? "")
        case .open:
            open(action.value ?? "")
            hidePanel()
        case .callback:
            host.send(.action(session: session.id, depth: depth, action: action.id))
        case .push:
            session.awaitingPush = true
            host.send(.action(session: session.id, depth: depth, action: action.id))
        }
    }

    func perform(builtin: Builtin) {
        switch builtin {
        case .reloadSpells:
            host.send(.reload)
            showToast(Toast(style: .info, title: "Reloading spells…", message: nil))
        case .openGrimoire:
            NSWorkspace.shared.open(CantripConfig.grimoireDirectory)
            hidePanel()
        case .openLog:
            NSWorkspace.shared.open(host.logURL)
            hidePanel()
        case .quit:
            NSApp.terminate(nil)
        }
    }

    private func run(_ command: WireCommand) {
        nextSession += 1
        let id = "s\(nextSession)"
        if command.mode == .noView {
            hidePanel()
            host.send(.run(session: id, command: command.id))
            return
        }
        endSession()
        session = Session(id: id, command: command)
        host.send(.run(session: id, command: command.id))
        focusToken += 1
    }

    func popFrame(_ session: Session) {
        let depth = session.frames.count - 1
        if depth <= 0 {
            endSession()
        } else {
            session.frames.removeLast()
            session.awaitingPush = false
            host.send(.pop(session: session.id, depth: depth))
        }
    }

    func endSession() {
        guard let session else { return }
        host.send(.close(session: session.id))
        self.session = nil
        actionsOpen = false
    }

    /// Back to the root search with an empty query.
    func reset() {
        endSession()
        rootQuery = ""
        rootSelection = 0
    }

    private func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        hidePanel()
        showHUD("Copied to clipboard")
    }

    func open(_ target: String) {
        let url: URL? =
            if target.hasPrefix("/") || target.hasPrefix("~") {
                URL(filePath: (target as NSString).expandingTildeInPath)
            } else {
                URL(string: target)
            }
        if let url { NSWorkspace.shared.open(url) }
    }

    private func bumpUsage(_ key: String) {
        usage[key, default: 0] += 1
        UserDefaults.standard.set(usage, forKey: "usage")
    }

    func showToast(_ toast: Toast) {
        self.toast = toast
        toastTask?.cancel()
        toastTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(toast.style == .failure ? 6 : 3))
            if !Task.isCancelled { self?.toast = nil }
        }
    }

}
