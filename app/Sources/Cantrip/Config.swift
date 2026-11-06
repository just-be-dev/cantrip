// ~/.config/cantrip/config.json
//
// {
//   "hotkey": "option+space",   // modifiers: cmd, option/alt, ctrl, shift
//   "bun": "/path/to/bun"       // optional; found automatically otherwise
// }

import Carbon
import Foundation

struct CantripConfig: Decodable {
    var hotkey: String = "option+space"
    var bun: String?

    static var directory: URL {
        FileManager.default.homeDirectoryForCurrentUser.appending(path: ".config/cantrip")
    }

    /// The grimoire: the folder of spells.
    static var grimoireDirectory: URL {
        if let env = ProcessInfo.processInfo.environment["CANTRIP_GRIMOIRE"] { return URL(filePath: env) }
        return directory.appending(path: "grimoire")
    }

    static func load() -> CantripConfig {
        let url = directory.appending(path: "config.json")
        guard let data = try? Data(contentsOf: url) else { return CantripConfig() }
        do {
            return try JSONDecoder().decode(CantripConfig.self, from: data)
        } catch {
            NSLog("Cantrip: ignoring invalid \(url.path): \(error)")
            return CantripConfig()
        }
    }

    private enum CodingKeys: String, CodingKey { case hotkey, bun }

    init() {}

    init(from decoder: Decoder) throws {
        let box = try decoder.container(keyedBy: CodingKeys.self)
        hotkey = try box.decodeIfPresent(String.self, forKey: .hotkey) ?? "option+space"
        bun = try box.decodeIfPresent(String.self, forKey: .bun)
    }

    /// Parses "cmd+shift+space" into a Carbon key code and modifier mask.
    var parsedHotkey: (keyCode: UInt32, modifiers: UInt32)? {
        var modifiers: UInt32 = 0
        var keyCode: UInt32?
        for part in hotkey.lowercased().split(separator: "+").map({ $0.trimmingCharacters(in: .whitespaces) }) {
            switch part {
            case "cmd", "command": modifiers |= UInt32(cmdKey)
            case "opt", "option", "alt": modifiers |= UInt32(optionKey)
            case "ctrl", "control": modifiers |= UInt32(controlKey)
            case "shift": modifiers |= UInt32(shiftKey)
            default: keyCode = Self.keyCodes[part]
            }
        }
        guard let keyCode else { return nil }
        return (keyCode, modifiers)
    }

    private static let keyCodes: [String: UInt32] = {
        var map: [String: UInt32] = [
            "space": UInt32(kVK_Space), "return": UInt32(kVK_Return), "enter": UInt32(kVK_Return),
            "tab": UInt32(kVK_Tab), "escape": UInt32(kVK_Escape), "`": UInt32(kVK_ANSI_Grave),
            ";": UInt32(kVK_ANSI_Semicolon), "/": UInt32(kVK_ANSI_Slash), ".": UInt32(kVK_ANSI_Period),
        ]
        let letters: [(String, Int)] = [
            ("a", kVK_ANSI_A), ("b", kVK_ANSI_B), ("c", kVK_ANSI_C), ("d", kVK_ANSI_D), ("e", kVK_ANSI_E),
            ("f", kVK_ANSI_F), ("g", kVK_ANSI_G), ("h", kVK_ANSI_H), ("i", kVK_ANSI_I), ("j", kVK_ANSI_J),
            ("k", kVK_ANSI_K), ("l", kVK_ANSI_L), ("m", kVK_ANSI_M), ("n", kVK_ANSI_N), ("o", kVK_ANSI_O),
            ("p", kVK_ANSI_P), ("q", kVK_ANSI_Q), ("r", kVK_ANSI_R), ("s", kVK_ANSI_S), ("t", kVK_ANSI_T),
            ("u", kVK_ANSI_U), ("v", kVK_ANSI_V), ("w", kVK_ANSI_W), ("x", kVK_ANSI_X), ("y", kVK_ANSI_Y),
            ("z", kVK_ANSI_Z), ("0", kVK_ANSI_0), ("1", kVK_ANSI_1), ("2", kVK_ANSI_2), ("3", kVK_ANSI_3),
            ("4", kVK_ANSI_4), ("5", kVK_ANSI_5), ("6", kVK_ANSI_6), ("7", kVK_ANSI_7), ("8", kVK_ANSI_8),
            ("9", kVK_ANSI_9),
        ]
        for (name, code) in letters { map[name] = UInt32(code) }
        return map
    }()
}
