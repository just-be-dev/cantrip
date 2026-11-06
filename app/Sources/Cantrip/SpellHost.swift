// Runs the bun spell host as a child process and talks JSON lines over stdio.

import AppKit
import Foundation

@MainActor
final class SpellHost {
    var onMessage: (HostMessage) -> Void = { _ in }
    /// Called whenever the host (re)starts, so stale sessions can be dropped.
    var onRestart: () -> Void = {}

    private var process: Process?
    private var stdin: FileHandle?
    private var quitting = false
    private var restartDelay: TimeInterval = 0.5
    private let config: CantripConfig

    init(config: CantripConfig) {
        self.config = config
    }

    var logURL: URL {
        FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/Logs/Cantrip/host.log")
    }

    func start() {
        guard let bun = Self.findBun(config: config) else {
            NSLog("Cantrip: could not find bun. Set \"bun\" in ~/.config/cantrip/config.json")
            onMessage(.error(session: "", title: "bun not found", message: "Install bun, or set \"bun\" in ~/.config/cantrip/config.json."))
            return
        }
        guard let entry = Self.findHostEntry() else {
            NSLog("Cantrip: could not find runtime/src/host/main.ts")
            return
        }

        let process = Process()
        process.executableURL = bun
        process.arguments = [entry.path]
        process.currentDirectoryURL = entry.deletingLastPathComponent()
        process.environment = Self.environment(bun: bun)

        let input = Pipe(), output = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = openLog()

        let reader = LineReader { [weak self] line in
            guard let data = line.data(using: .utf8) else { return }
            do {
                let message = try JSONDecoder().decode(HostMessage.self, from: data)
                DispatchQueue.main.async { MainActor.assumeIsolated { self?.onMessage(message) } }
            } catch {
                NSLog("Cantrip: undecodable host line: \(line.prefix(300)) (\(error))")
            }
        }
        output.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty { handle.readabilityHandler = nil } else { reader.feed(data) }
        }
        process.terminationHandler = { [weak self] proc in
            let status = proc.terminationStatus
            DispatchQueue.main.async { MainActor.assumeIsolated { self?.didTerminate(status: status) } }
        }

        do {
            try process.run()
            self.process = process
            self.stdin = input.fileHandleForWriting
            NSLog("Cantrip: started host \(bun.path) \(entry.path) (pid \(process.processIdentifier))")
        } catch {
            NSLog("Cantrip: failed to start host: \(error)")
        }
    }

    func send(_ message: AppMessage) {
        guard let stdin, let data = try? JSONSerialization.data(withJSONObject: message.json) else { return }
        do {
            try stdin.write(contentsOf: data + Data([0x0A]))
        } catch {
            NSLog("Cantrip: write to host failed: \(error)")
        }
    }

    /// Restart the host. Every running spell is interrupted.
    func restart() {
        process?.terminate()  // didTerminate starts a new one
    }

    func stop() {
        quitting = true
        try? stdin?.close()
        process?.terminate()
    }

    private func didTerminate(status: Int32) {
        process = nil
        stdin = nil
        guard !quitting else { return }
        NSLog("Cantrip: host exited (\(status)); restarting in \(restartDelay)s")
        onRestart()
        let delay = restartDelay
        restartDelay = min(restartDelay * 2, 30)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            MainActor.assumeIsolated {
                self?.start()
            }
        }
        // A host that stays up for a while resets the backoff.
        DispatchQueue.main.asyncAfter(deadline: .now() + delay + 60) { [weak self] in
            MainActor.assumeIsolated {
                if self?.process?.isRunning == true { self?.restartDelay = 0.5 }
            }
        }
    }

    private func openLog() -> FileHandle? {
        let fm = FileManager.default
        try? fm.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        if !fm.fileExists(atPath: logURL.path) { fm.createFile(atPath: logURL.path, contents: nil) }
        let handle = try? FileHandle(forWritingTo: logURL)
        _ = try? handle?.seekToEnd()
        return handle
    }

    // MARK: Locating bun and the runtime

    static func findBun(config: CantripConfig) -> URL? {
        let fm = FileManager.default
        let home = fm.homeDirectoryForCurrentUser.path
        var candidates: [String] = []
        if let configured = config.bun { candidates.append((configured as NSString).expandingTildeInPath) }
        if let env = ProcessInfo.processInfo.environment["CANTRIP_BUN"] { candidates.append(env) }
        // The project's pinned bun, recorded by scripts/bundle.sh at build time.
        if let pinned = Bundle.main.object(forInfoDictionaryKey: "CantripBun") as? String { candidates.append(pinned) }
        candidates += [
            "\(home)/.bun/bin/bun",
            "\(home)/.local/share/mise/shims/bun",
            "/opt/homebrew/bin/bun",
            "/usr/local/bin/bun",
        ]
        if let found = candidates.first(where: { fm.isExecutableFile(atPath: $0) }) {
            return URL(filePath: found)
        }
        // Last resort: ask a login shell.
        let shell = Process()
        shell.executableURL = URL(filePath: "/bin/zsh")
        shell.arguments = ["-lc", "command -v bun"]
        let out = Pipe()
        shell.standardOutput = out
        shell.standardError = FileHandle.nullDevice
        guard (try? shell.run()) != nil else { return nil }
        shell.waitUntilExit()
        let path = String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return path.isEmpty ? nil : URL(filePath: path)
    }

    /// The host entry point: `$CANTRIP_RUNTIME`, the app bundle's Resources, or
    /// (for `swift run` during development) a `runtime/` dir above the executable.
    static func findHostEntry() -> URL? {
        let fm = FileManager.default
        let relative = "src/host/main.ts"
        if let env = ProcessInfo.processInfo.environment["CANTRIP_RUNTIME"] {
            return URL(filePath: env).appending(path: relative)
        }
        if let resources = Bundle.main.resourceURL {
            let bundled = resources.appending(path: "runtime").appending(path: relative)
            if fm.fileExists(atPath: bundled.path) { return bundled }
        }
        var dir = URL(filePath: CommandLine.arguments[0]).resolvingSymlinksInPath().deletingLastPathComponent()
        while dir.path != "/" {
            let candidate = dir.appending(path: "runtime").appending(path: relative)
            if fm.fileExists(atPath: candidate.path) { return candidate }
            dir = dir.deletingLastPathComponent()
        }
        return nil
    }

    /// GUI apps get a bare PATH. Give spells the usual tool locations so
    /// `git`, `brew`, `gh`, etc. resolve like they do in a terminal.
    static func environment(bun: URL) -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let extra = [
            bun.deletingLastPathComponent().path,
            "\(home)/.bun/bin",
            "\(home)/.local/bin",
            "\(home)/.local/share/mise/shims",
            "/opt/homebrew/bin",
            "/opt/homebrew/sbin",
            "/usr/local/bin",
        ]
        let existing = (env["PATH"] ?? "/usr/bin:/bin:/usr/sbin:/sbin").split(separator: ":").map(String.init)
        env["PATH"] = (extra + existing).reduce(into: [String]()) { acc, entry in if !acc.contains(entry) { acc.append(entry) } }
            .joined(separator: ":")
        env["NO_COLOR"] = "1"
        return env
    }
}

/// Splits a byte stream into UTF-8 lines. Only touched from the pipe's
/// readability handler, which runs serially.
private final class LineReader: @unchecked Sendable {
    private var buffer = Data()
    private let onLine: (String) -> Void

    init(onLine: @escaping (String) -> Void) {
        self.onLine = onLine
    }

    func feed(_ data: Data) {
        buffer.append(data)
        while let newline = buffer.firstIndex(of: 0x0A) {
            let lineData = buffer[buffer.startIndex..<newline]
            buffer.removeSubrange(buffer.startIndex...newline)
            let line = String(decoding: lineData, as: UTF8.self)
            if !line.trimmingCharacters(in: .whitespaces).isEmpty { onLine(line) }
        }
    }
}
