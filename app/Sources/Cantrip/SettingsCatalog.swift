// System Settings panes, so "displays" finds Displays like it does in Spotlight.
//
// Since macOS 13 each pane is an ExtensionKit extension in
// /System/Library/ExtensionKit/Extensions whose extension point is
// com.apple.Settings.extension.ui, opened with x-apple.systempreferences:<bundle id>.
// Third-party panes are still legacy .prefPane bundles, opened by path.

import Foundation
import IOKit.ps

struct SettingsPane: Identifiable, Hashable, Sendable {
    let name: String
    /// Other names it goes by (Battery vs. Energy, Login Items & Extensions).
    let aliases: [String]
    /// The bundle on disk.
    let path: String
    /// What to open: a URL, or the bundle's path for a legacy pane.
    let target: String
    /// A UTType whose icon to show instead of the bundle's own.
    let iconType: String?
    var id: String { path }
}

enum SettingsCatalog {
    static func scan() async -> [SettingsPane] {
        await Task.detached(priority: .utility) { scanSync() }.value
    }

    private static func scanSync() -> [SettingsPane] {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let panes = extensions(in: "/System/Library/ExtensionKit/Extensions")
            + legacyPanes(in: "/Library/PreferencePanes")
            + legacyPanes(in: "\(home)/Library/PreferencePanes")
        var seen = Set<String>()
        return panes
            .filter { seen.insert($0.name.lowercased()).inserted }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    private static func extensions(in dir: String) -> [SettingsPane] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
        return names.filter { $0.hasSuffix(".appex") }.compactMap { name in
            let path = "\(dir)/\(name)"
            guard let bundle = Bundle(url: URL(filePath: path)),
                let id = bundle.bundleIdentifier,
                let info = bundle.infoDictionary,
                let attributes = info["EXAppExtensionAttributes"] as? [String: Any],
                attributes["EXExtensionPointIdentifier"] as? String == "com.apple.Settings.extension.ui"
            else { return nil }
            let settings = attributes["SettingsExtensionAttributes"] as? [String: Any] ?? [:]

            // Some panes go by a different name depending on the Mac (Battery on a laptop, Energy on a desktop).
            let representations = (settings["representations"] as? [[String: Any]] ?? [])
                .filter { $0["hidden"] as? Bool != true }
                .compactMap { rep -> (name: String, icon: String?, applies: Bool?)? in
                    guard let key = rep["sidebar-name"] as? String else { return nil }
                    return (
                        bundle.localizedString(forKey: key, value: key, table: nil),
                        rep["sidebar-iconUTTypeIdentifier"] as? String,
                        evaluate(rep["predicate"] as? String))
                }
            let localized = bundle.localizedInfoDictionary
            let bundleName = [
                localized?["CFBundleDisplayName"], localized?["CFBundleName"],
                info["CFBundleDisplayName"], info["CFBundleName"],
            ].lazy.compactMap { $0 as? String }.first { !$0.isEmpty }

            let chosen = representations.first { $0.applies == true }
            let name: String
            if let chosen {
                name = chosen.name
            } else if let bundleName, !looksLikeIdentifier(bundleName) {
                name = bundleName
            } else if let first = representations.first {
                name = first.name
            } else {
                // Only a code name to go on (e.g. HeadphoneSettingsExtension): not worth showing.
                return nil
            }
            var aliases = representations.map(\.name)
            if let alt = settings["alt_name"] as? String { aliases.append(alt) }
            if let bundleName, !looksLikeIdentifier(bundleName) { aliases.append(bundleName) }
            // "Wi‑Fi" is spelled with a non-breaking hyphen; let "wi-fi" find it.
            aliases += ([name] + aliases).map { $0.replacingOccurrences(of: "\u{2011}", with: "-") }
            return SettingsPane(
                name: name, aliases: Array(Set(aliases).subtracting([name])).sorted(), path: path,
                target: "x-apple.systempreferences:\(id)", iconType: chosen?.icon)
        }
    }

    private static func legacyPanes(in dir: String) -> [SettingsPane] {
        let fm = FileManager.default
        let names = (try? fm.contentsOfDirectory(atPath: dir)) ?? []
        return names.filter { $0.hasSuffix(".prefPane") }.map { name in
            let path = "\(dir)/\(name)"
            let display = fm.displayName(atPath: path).replacingOccurrences(of: ".prefPane", with: "")
            return SettingsPane(name: display, aliases: [], path: path, target: path, iconType: nil)
        }
    }

    /// "HeadphoneSettingsExtension", "PowerPreferences": a name nobody would search for.
    private static func looksLikeIdentifier(_ name: String) -> Bool {
        guard !name.contains(" ") else { return false }
        return ["Extension", "Settings", "Preferences", "Ext", "Pane"].contains { name.hasSuffix($0) && name != $0 }
    }

    /// The few representation predicates we can answer; nil for the rest.
    private static func evaluate(_ predicate: String?) -> Bool? {
        guard let predicate else { return nil }
        let parts = predicate.split(separator: " ")
        guard parts.count == 3, parts[2] == "YES", parts[1] == "==" || parts[1] == "!=" else { return nil }
        let value: Bool
        switch parts[0] {
        case "powersource.battery": value = hasBattery
        default: return nil
        }
        return parts[1] == "==" ? value : !value
    }

    private static let hasBattery: Bool = {
        guard let info = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
            let sources = IOPSCopyPowerSourcesList(info)?.takeRetainedValue() as? [CFTypeRef]
        else { return false }
        return sources.contains { source in
            let description = IOPSGetPowerSourceDescription(info, source)?.takeUnretainedValue() as? [String: Any]
            return description?[kIOPSTypeKey] as? String == kIOPSInternalBatteryType
        }
    }()
}
