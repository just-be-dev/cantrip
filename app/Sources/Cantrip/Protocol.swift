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

    private enum Keys: String, CodingKey {
        case type, commands, errors, session, depth, view, loading, style, title, message, text, target, rendered
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

    var json: [String: Any] {
        switch self {
        case let .run(session, command): ["type": "run", "session": session, "command": command]
        case let .query(session, depth, text): ["type": "query", "session": session, "depth": depth, "text": text]
        case let .action(session, depth, action): ["type": "action", "session": session, "depth": depth, "action": action]
        case let .pop(session, depth): ["type": "pop", "session": session, "depth": depth]
        case let .close(session): ["type": "close", "session": session]
        case .reload: ["type": "reload"]
        }
    }
}
