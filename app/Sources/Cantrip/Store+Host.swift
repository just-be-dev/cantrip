import AppKit

extension Store {
    // MARK: Host messages

    func receive(_ message: HostMessage) {
        switch message {
        case let .commands(commands, errors):
            self.commands = commands
            self.loadErrors = errors
            if !errors.isEmpty {
                showToast(Toast(style: .failure, title: "\(errors.count) spell(s) failed to load", message: errors.joined(separator: "\n")))
            } else if restingSince != nil {
                showToast(Toast(style: .success, title: "Rested", message: "\(commands.count) spells ready"))
            }
            restingSince = nil
        case let .render(id, depth, view):
            guard let session, session.id == id else { return }
            if depth < session.frames.count {
                session.frames[depth].view = view
            } else if depth == session.frames.count {
                session.frames.append(Frame(view: view))
                session.awaitingPush = false
                actionsOpen = false
            }
            let count = rows.count
            if selection >= count { selection = max(0, count - 1) }
        case let .pop(id, depth):
            guard let session, session.id == id else { return }
            session.frames = Array(session.frames.prefix(depth))
            if session.frames.isEmpty { endSession() }
        case let .closed(id):
            if session?.id == id { self.session = nil }
        case let .loading(id, loading):
            if session?.id == id { session?.hostLoading = loading }
        case let .toast(_, style, title, message):
            showToast(Toast(style: style, title: title, message: message))
        case let .hud(message):
            hidePanel()
            showHUD(message)
        case .hide:
            hidePanel()
        case let .clipboard(text):
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        case let .open(target):
            open(target)
        case let .done(id, _):
            if session?.id == id {
                session = nil
                hidePanel()
            }
        case let .error(id, title, message):
            if let session, session.id == id {
                session.error = (title, message)
                session.awaitingPush = false
            } else {
                showHUD("⚠︎ \(title)")
                NSLog("Cantrip: \(title)\n\(message)")
            }
        }
    }

    /// The host restarted: every session it knew about is gone.
    func hostRestarted() {
        if session != nil, restingSince == nil {
            session = nil
            showToast(Toast(style: .failure, title: "Concentration broken", message: "The spell engine restarted, so the running spell stopped."))
        }
    }
}
