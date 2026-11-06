// The `ring` menu bar icon: a circle that fills clockwise from 12 o'clock.
// Ported from the Claude Usage menu bar app.

import AppKit
import QuartzCore

enum RingIcon {
    struct Frame {
        /// 0–1 to draw, or nil when there's no data yet (dashed track).
        var value: Double?
        /// Rotation of the busy segment in radians, or nil when not busy.
        var spinnerAngle: CGFloat?
        var badge = false
        var tone: WireTone = .normal
    }

    static func image(_ frame: Frame) -> NSImage {
        let size = NSSize(width: 18, height: 18)
        let tone = frame.tone
        let image = NSImage(size: size, flipped: false) { rect in
            let center = NSPoint(x: rect.midX, y: rect.midY)
            let radius: CGFloat = 6.8
            let lineWidth: CGFloat = 2.4
            let base: NSColor =
                switch tone {
                case .normal: .black  // template: the system tints it for light/dark
                case .warning: .systemOrange
                case .critical: .systemRed
                }

            // Track: the full circle, faint. Dashed when there's no data at all.
            let track = NSBezierPath()
            track.appendArc(withCenter: center, radius: radius, startAngle: 0, endAngle: 360)
            track.lineWidth = lineWidth
            if frame.value == nil { track.setLineDash([1.6, 2.2], count: 2, phase: 0) }
            base.withAlphaComponent(0.28).setStroke()
            track.stroke()

            if let value = frame.value, value > 0.005 {
                let sweep = 360 * CGFloat(min(value, 1))
                let arc = NSBezierPath()
                arc.appendArc(withCenter: center, radius: radius, startAngle: 90, endAngle: 90 - sweep, clockwise: true)
                arc.lineWidth = lineWidth
                arc.lineCapStyle = .round
                base.setStroke()
                arc.stroke()
            }

            if let angle = frame.spinnerAngle {
                let startDeg = 90 - angle * 180 / .pi
                let spin = NSBezierPath()
                spin.appendArc(withCenter: center, radius: radius, startAngle: startDeg, endAngle: startDeg - 70, clockwise: true)
                spin.lineWidth = lineWidth
                spin.lineCapStyle = .round
                base.withAlphaComponent(frame.value == nil ? 1 : 0.55).setStroke()
                spin.stroke()
            }

            if frame.badge {
                base.setFill()
                NSBezierPath(ovalIn: NSRect(x: center.x - 1.8, y: center.y - 1.8, width: 3.6, height: 3.6)).fill()
            }
            return true
        }
        image.isTemplate = tone == .normal
        return image
    }
}

/// Drives a status item button's ring at display rate while something moves:
/// eases between values (filling up from empty on the first one), spins while
/// busy, and pulses the button's layer when asked. The timer stops at rest.
@MainActor
final class RingAnimator {
    private weak var button: NSStatusBarButton?

    private var shown: Double?
    private var from: Double = 0
    private var target: Double?
    private var hasTarget = false
    private var sweepStart: CFTimeInterval = 0
    private let sweepDuration: CFTimeInterval = 1.1

    private var busy = false
    private var busyStart: CFTimeInterval = 0
    private var badge = false
    private var tone: WireTone = .normal
    private var pulse = false

    private var timer: Timer?

    init(button: NSStatusBarButton) {
        self.button = button
    }

    func update(value: Double?, tone: WireTone, busy: Bool, pulse: Bool, badge: Bool) {
        let now = CACurrentMediaTime()
        if !hasTarget || value != target {
            from = shown ?? 0
            target = value
            hasTarget = true
            sweepStart = now
            if value == nil { shown = nil }
        }
        if busy && !self.busy { busyStart = now }
        self.busy = busy
        self.badge = badge
        self.tone = tone
        self.pulse = pulse
        render(now: now)
        if needsAnimation(now: now) { startTimer() }
        updatePulse()
    }

    /// Stop animating and clear the pulse, e.g. when the item switches to another icon.
    func stop() {
        timer?.invalidate()
        timer = nil
        button?.layer?.removeAnimation(forKey: "pulse")
    }

    /// A Core Animation opacity loop: it runs in the render server, so it's cheap even for hours.
    private func updatePulse() {
        guard let button else { return }
        button.wantsLayer = true
        guard let layer = button.layer else { return }
        let shouldPulse = pulse && !reduceMotion
        let isPulsing = layer.animation(forKey: "pulse") != nil
        if shouldPulse && !isPulsing {
            let anim = CABasicAnimation(keyPath: "opacity")
            anim.fromValue = 1.0
            anim.toValue = 0.35
            anim.duration = 0.8
            anim.autoreverses = true
            anim.repeatCount = .infinity
            anim.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            layer.add(anim, forKey: "pulse")
        } else if !shouldPulse && isPulsing {
            layer.removeAnimation(forKey: "pulse")
        }
    }

    private var reduceMotion: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    private func needsAnimation(now: CFTimeInterval) -> Bool {
        if reduceMotion { return false }
        let sweeping = target != nil && now - sweepStart < sweepDuration
        return sweeping || busy
    }

    private func startTimer() {
        guard timer == nil else { return }
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        // .common so it keeps animating while the menu is open.
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func tick() {
        let now = CACurrentMediaTime()
        render(now: now)
        if !needsAnimation(now: now) {
            timer?.invalidate()
            timer = nil
            render(now: now)
        }
    }

    private func render(now: CFTimeInterval) {
        if let target {
            if reduceMotion {
                shown = target
            } else {
                let progress = min(1, max(0, (now - sweepStart) / sweepDuration))
                shown = from + (target - from) * (1 - pow(1 - progress, 3))
            }
        }
        var frame = RingIcon.Frame(value: shown, badge: badge, tone: tone)
        if busy && !reduceMotion {
            frame.spinnerAngle = CGFloat((now - busyStart) * 2 * .pi / 0.9)
        }
        button?.image = RingIcon.image(frame)
    }
}
