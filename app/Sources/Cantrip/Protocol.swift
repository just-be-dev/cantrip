// The wire protocol with the bun spell host: one JSON object per line.
// Keep in sync with runtime/src/host/Protocol.ts.

import Foundation

struct WireCommand: Decodable, Identifiable, Hashable {
    enum Mode: String, Decodable { case view, noView = "no-view" }
    let id: String
    let title: String
    let subtitle: String?
    let icon: String?
    let keywords: [String]
    let mode: Mode
}

struct WireAction: Decodable, Identifiable, Hashable {
    enum Kind: String, Decodable { case callback, push, copy, open }
    let id: String
    let title: String
    let icon: String?
    let kind: Kind
    let value: String?
}

struct WireListItem: Decodable, Identifiable, Hashable {
    let id: String
    let title: String
    let subtitle: String?
    let icon: String?
    let accessories: [String]
    let keywords: [String]
    let actions: [WireAction]
}

struct WireMetadata: Decodable, Hashable {
    let label: String
    let value: String
}

enum WireView: Decodable {
    case list(placeholder: String?, filtering: Bool, emptyText: String?, items: [WireListItem])
    case detail(markdown: String, metadata: [WireMetadata], actions: [WireAction])

    private enum Keys: String, CodingKey {
        case kind, placeholder, filtering, emptyText, items, markdown, metadata, actions
    }

    init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Keys.self)
        switch try box.decode(String.self, forKey: .kind) {
        case "list":
            self = .list(
                placeholder: try box.decodeIfPresent(String.self, forKey: .placeholder),
                filtering: try box.decode(Bool.self, forKey: .filtering),
                emptyText: try box.decodeIfPresent(String.self, forKey: .emptyText),
                items: try box.decode([WireListItem].self, forKey: .items))
        case "detail":
            self = .detail(
                markdown: try box.decode(String.self, forKey: .markdown),
                metadata: try box.decode([WireMetadata].self, forKey: .metadata),
                actions: try box.decode([WireAction].self, forKey: .actions))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: box, debugDescription: "Unknown view kind \(other)")
        }
    }
}

enum ToastStyle: String, Decodable { case success, failure, info }

enum WireTone: String, Decodable { case normal, warning, critical }

enum WireBarIcon: Decodable, Equatable {
    case image(String)
    /// `value` is 0–1, or nil for "no data yet".
    case ring(value: Double?, tone: WireTone, busy: Bool, pulse: Bool, badge: Bool)

    var imageName: String? {
        if case let .image(name) = self { return name }
        return nil
    }

    private enum Keys: String, CodingKey { case kind, value, tone, busy, pulse, badge }

    init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Keys.self)
        switch try box.decode(String.self, forKey: .kind) {
        case "image":
            self = .image(try box.decode(String.self, forKey: .value))
        case "ring":
            self = .ring(
                value: try box.decodeIfPresent(Double.self, forKey: .value),
                tone: try box.decode(WireTone.self, forKey: .tone),
                busy: try box.decode(Bool.self, forKey: .busy),
                pulse: try box.decode(Bool.self, forKey: .pulse),
                badge: try box.decode(Bool.self, forKey: .badge))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: box, debugDescription: "Unknown icon kind \(other)")
        }
    }
}

struct WireMenuItem: Decodable, Equatable {
    enum Action: String, Decodable { case none, callback, copy, open }
    let id: String
    let title: String
    let subtitle: String?
    let icon: String?
    let accessory: String?
    let progress: Double?
    let tone: WireTone
    let checked: Bool
    let enabled: Bool
    let key: String?
    let action: Action
    let value: String?
    let submenu: [WireMenuEntry]
}

indirect enum WireMenuEntry: Decodable, Equatable {
    case separator
    case header(String)
    case item(WireMenuItem)

    private enum Keys: String, CodingKey { case kind, title }

    init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Keys.self)
        switch try box.decode(String.self, forKey: .kind) {
        case "separator": self = .separator
        case "header": self = .header(try box.decode(String.self, forKey: .title))
        case "item": self = .item(try WireMenuItem(from: decoder))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .kind, in: box, debugDescription: "Unknown menu entry \(other)")
        }
    }
}

struct WireStatusItem: Decodable, Equatable {
    let title: String
    let daemonIcon: String?
    let icon: WireBarIcon
    let label: String?
    let tooltip: String?
    let menu: [WireMenuEntry]
}

enum HostMessage: Decodable {
    case commands([WireCommand], errors: [String])
    case render(session: String, depth: Int, view: WireView)
    case pop(session: String, depth: Int)
    case closed(session: String)
    case loading(session: String, loading: Bool)
    case toast(session: String, style: ToastStyle, title: String, message: String?)
    case hud(String)
    case hide
    case clipboard(String)
    case open(String)
    case done(session: String, rendered: Bool)
    case error(session: String, title: String, message: String)
    case status(daemon: String, item: WireStatusItem)
    case statusRemoved(daemon: String)

    private enum Keys: String, CodingKey {
        case type, commands, errors, session, depth, view, loading, style, title, message, text, target, rendered, daemon, item
    }

    init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: Keys.self)
        func session() throws -> String { try box.decode(String.self, forKey: .session) }
        switch try box.decode(String.self, forKey: .type) {
        case "commands":
            self = .commands(try box.decode([WireCommand].self, forKey: .commands), errors: try box.decode([String].self, forKey: .errors))
        case "render":
            self = .render(session: try session(), depth: try box.decode(Int.self, forKey: .depth), view: try box.decode(WireView.self, forKey: .view))
        case "pop":
            self = .pop(session: try session(), depth: try box.decode(Int.self, forKey: .depth))
        case "closed":
            self = .closed(session: try session())
        case "loading":
            self = .loading(session: try session(), loading: try box.decode(Bool.self, forKey: .loading))
        case "toast":
            self = .toast(
                session: try session(), style: try box.decode(ToastStyle.self, forKey: .style),
                title: try box.decode(String.self, forKey: .title), message: try box.decodeIfPresent(String.self, forKey: .message))
        case "hud":
            self = .hud(try box.decode(String.self, forKey: .message))
        case "hide":
            self = .hide
        case "clipboard":
            self = .clipboard(try box.decode(String.self, forKey: .text))
        case "open":
            self = .open(try box.decode(String.self, forKey: .target))
        case "done":
            self = .done(session: try session(), rendered: try box.decode(Bool.self, forKey: .rendered))
        case "error":
            self = .error(session: try session(), title: try box.decode(String.self, forKey: .title), message: try box.decode(String.self, forKey: .message))
        case "status":
            self = .status(daemon: try box.decode(String.self, forKey: .daemon), item: try box.decode(WireStatusItem.self, forKey: .item))
        case "statusRemoved":
            self = .statusRemoved(daemon: try box.decode(String.self, forKey: .daemon))
        case let other:
            throw DecodingError.dataCorruptedError(forKey: .type, in: box, debugDescription: "Unknown message \(other)")
        }
    }
}

enum AppMessage {
    case run(session: String, command: String)
    case query(session: String, depth: Int, text: String)
    case action(session: String, depth: Int, action: String)
    case pop(session: String, depth: Int)
    case close(session: String)
    case reload
    case menu(daemon: String, item: String)
    case menuOpened(daemon: String)

    var json: [String: Any] {
        switch self {
        case let .run(session, command): ["type": "run", "session": session, "command": command]
        case let .query(session, depth, text): ["type": "query", "session": session, "depth": depth, "text": text]
        case let .action(session, depth, action): ["type": "action", "session": session, "depth": depth, "action": action]
        case let .pop(session, depth): ["type": "pop", "session": session, "depth": depth]
        case let .close(session): ["type": "close", "session": session]
        case .reload: ["type": "reload"]
        case let .menu(daemon, item): ["type": "menu", "daemon": daemon, "item": item]
        case let .menuOpened(daemon): ["type": "menuOpened", "daemon": daemon]
        }
    }
}
