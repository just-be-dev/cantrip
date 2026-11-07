import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// A spell icon string: an SF Symbol name, an emoji, or a file path (Finder icon).
enum Icon: Hashable {
    case symbol(String)
    case emoji(String)
    case file(String)
    /// The system icon for a UTType identifier (System Settings uses these for some panes).
    case contentType(String)

    init(_ raw: String?) {
        guard let raw, !raw.isEmpty else {
            self = .symbol("sparkles")
            return
        }
        if raw.hasPrefix("/") || raw.hasPrefix("~") {
            self = .file((raw as NSString).expandingTildeInPath)
        } else if raw.unicodeScalars.contains(where: { $0.properties.isEmojiPresentation || $0.value > 0x2000 }) {
            self = .emoji(raw)
        } else {
            self = .symbol(raw)
        }
    }
}

@MainActor
private enum FileIconCache {
    static var images: [String: NSImage] = [:]

    static func image(_ path: String) -> NSImage {
        if let cached = images[path] { return cached }
        let image = NSWorkspace.shared.icon(forFile: path)
        images[path] = image
        return image
    }

    static func image(type identifier: String) -> NSImage {
        let key = "type:\(identifier)"
        if let cached = images[key] { return cached }
        let image = NSWorkspace.shared.icon(for: UTType(identifier) ?? .data)
        images[key] = image
        return image
    }
}

struct IconView: View {
    let icon: Icon
    var size: CGFloat = 20

    var body: some View {
        switch icon {
        case let .symbol(name):
            Image(systemName: name)
                .font(.system(size: size * 0.7, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: size, height: size)
        case let .emoji(text):
            Text(text)
                .font(.system(size: size * 0.8))
                .frame(width: size, height: size)
        case let .file(path):
            Image(nsImage: FileIconCache.image(path))
                .resizable()
                .frame(width: size, height: size)
        case let .contentType(identifier):
            Image(nsImage: FileIconCache.image(type: identifier))
                .resizable()
                .frame(width: size, height: size)
        }
    }
}
