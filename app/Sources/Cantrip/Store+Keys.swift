import AppKit

extension Store {
    // MARK: Keys

    /// Returns true when the key was handled.
    func handleKey(_ event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let cmd = mods.contains(.command), ctrl = mods.contains(.control)
        switch Int(event.keyCode) {
        case 125:  // down
            move(1); return true
        case 126:  // up
            move(-1); return true
        case 45 where ctrl:  // ctrl-n
            move(1); return true
        case 35 where ctrl:  // ctrl-p
            move(-1); return true
        case 36, 76:  // return
            submit(secondary: cmd)
            return true
        case 40 where cmd:  // ⌘K
            actionsOpen.toggle()
            actionsSelection = 0
            return true
        case 15 where cmd && session == nil:  // ⌘R
            perform(builtin: .reloadGrimoire); return true
        case 53:  // escape
            escape(); return true
        default:
            return false
        }
    }

    /// ↵ runs the first action, ⌘↵ the second; with ⌘K open, the highlighted one.
    func submit(secondary: Bool = false) {
        let actions = currentActions
        let index = actionsOpen ? actionsSelection : (secondary ? 1 : 0)
        if actions.indices.contains(index) { perform(actions[index]) }
    }

    private func move(_ delta: Int) {
        if actionsOpen {
            let count = currentActions.count
            guard count > 0 else { return }
            actionsSelection = (actionsSelection + delta + count) % count
        } else {
            let count = rows.count
            guard count > 0 else { return }
            selection = max(0, min(count - 1, selection + delta))
        }
    }

    private func escape() {
        if actionsOpen {
            actionsOpen = false
        } else if let session {
            popFrame(session)
        } else if !query.isEmpty {
            query = ""
        } else {
            hidePanel()
        }
    }
}
