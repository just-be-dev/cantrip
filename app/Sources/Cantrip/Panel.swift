// The floating command bar window, plus a transient HUD.

import AppKit
import SwiftUI

/// A borderless panel that takes keyboard focus without activating Cantrip,
/// so the app you were in stays frontmost (like Spotlight).
final class CommandPanel: NSPanel {
    init(size: NSSize) {
        super.init(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel, .fullSizeContentView],
            backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        hidesOnDeactivate = false
        isMovableByWindowBackground = true
        isReleasedWhenClosed = false
        animationBehavior = .utilityWindow
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}

@MainActor
final class PanelController: NSObject, NSWindowDelegate {
    private let panel: CommandPanel
    private let store: Store
    private var keyMonitor: Any?
    private var hiddenAt: Date?
    private let hud = HUD()

    static let size = NSSize(width: 760, height: 480)

    init(store: Store) {
        self.store = store
        panel = CommandPanel(size: Self.size)
        super.init()
        let hosting = NSHostingView(rootView: RootView(store: store))
        hosting.frame = NSRect(origin: .zero, size: Self.size)
        hosting.autoresizingMask = [.width, .height]
        panel.contentView = hosting
        panel.delegate = self

        store.hidePanel = { [weak self] in self?.hide() }
        store.showHUD = { [weak self] message in self?.hud.show(message) }

        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.panel else { return event }
            return self.store.handleKey(event) ? nil : event
        }
    }

    var isVisible: Bool { panel.isVisible }

    func toggle() {
        if panel.isVisible && panel.isKeyWindow { hide() } else { show() }
    }

    func show() {
        // After a while away, start fresh at the root search (like Raycast).
        if let hiddenAt, Date().timeIntervalSince(hiddenAt) > 90 { store.reset() }
        position()
        panel.makeKeyAndOrderFront(nil)
        store.focusToken += 1
    }

    func hide() {
        guard panel.isVisible else { return }
        store.actionsOpen = false
        panel.orderOut(nil)
        hiddenAt = Date()
    }

    /// Centered horizontally, a bit above center, on the screen with the mouse.
    private func position() {
        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        guard let frame = screen?.visibleFrame else { return }
        let origin = NSPoint(
            x: frame.midX - Self.size.width / 2,
            y: frame.minY + frame.height * 0.62 - Self.size.height / 2)
        panel.setFrame(NSRect(origin: origin, size: Self.size), display: false)
    }

    /// Set CANTRIP_KEEP_OPEN=1 to keep the bar up when it loses focus (handy while debugging spells).
    private let keepOpen = ProcessInfo.processInfo.environment["CANTRIP_KEEP_OPEN"] != nil

    func windowDidResignKey(_ notification: Notification) {
        if !keepOpen { hide() }
    }
}

/// A small message that fades in over the screen and goes away by itself.
@MainActor
final class HUD {
    private var window: NSPanel?
    private var hideTask: Task<Void, Never>?

    func show(_ message: String) {
        let view = NSHostingView(rootView: HUDView(message: message))
        let size = view.fittingSize
        let panel = window ?? NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        panel.contentView = view
        window = panel

        let mouse = NSEvent.mouseLocation
        let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
        if let frame = screen?.visibleFrame {
            panel.setFrame(
                NSRect(x: frame.midX - size.width / 2, y: frame.minY + frame.height * 0.18, width: size.width, height: size.height),
                display: true)
        }
        panel.alphaValue = 1
        panel.orderFrontRegardless()

        hideTask?.cancel()
        hideTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(1.6))
            guard !Task.isCancelled, let window = self?.window else { return }
            await NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.25
                window.animator().alphaValue = 0
            }
            if !Task.isCancelled { window.orderOut(nil) }
        }
    }
}

private struct HUDView: View {
    let message: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "wand.and.sparkles").foregroundStyle(.tint)
            Text(message).font(.system(size: 13, weight: .medium)).lineLimit(2)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(.white.opacity(0.12)))
        .fixedSize()
        .padding(12)
    }
}
