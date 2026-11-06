// Installed applications, found by scanning the usual folders.

import Foundation

struct AppEntry: Identifiable, Hashable, Sendable {
    let name: String
    let path: String
    var id: String { path }
}

enum AppCatalog {
    static func scan() async -> [AppEntry] {
        await Task.detached(priority: .utility) { scanSync() }.value
    }

    private static func scanSync() -> [AppEntry] {
        let fm = FileManager.default
        let home = fm.homeDirectoryForCurrentUser.path
        let roots = [
            "/Applications",
            "/System/Applications",
            "/System/Applications/Utilities",
            "/System/Library/CoreServices/Applications",
            "\(home)/Applications",
        ]
        var seen = Set<String>()
        var apps: [AppEntry] = []

        func visit(_ dir: String, depth: Int) {
            guard let names = try? fm.contentsOfDirectory(atPath: dir) else { return }
            for name in names where !name.hasPrefix(".") {
                let path = "\(dir)/\(name)"
                if name.hasSuffix(".app") {
                    let display = fm.displayName(atPath: path).replacingOccurrences(of: ".app", with: "")
                    if seen.insert(display.lowercased()).inserted {
                        apps.append(AppEntry(name: display, path: path))
                    }
                } else if depth > 0 {
                    var isDir: ObjCBool = false
                    if fm.fileExists(atPath: path, isDirectory: &isDir), isDir.boolValue {
                        visit(path, depth: depth - 1)
                    }
                }
            }
        }
        for root in roots { visit(root, depth: 1) }
        // Finder lives somewhere special.
        let finder = "/System/Library/CoreServices/Finder.app"
        if seen.insert("finder").inserted { apps.append(AppEntry(name: "Finder", path: finder)) }
        return apps.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }
}
