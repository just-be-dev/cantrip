import SwiftUI

struct FooterView: View {
    let store: Store

    var body: some View {
        HStack(spacing: 10) {
            leading
            Spacer()
            if let primary = store.currentActions.first {
                KeyHint(title: primary.title, keys: "↵")
                Divider().frame(height: 14)
            }
            KeyHint(title: "Actions", keys: "⌘K")
        }
        .padding(.horizontal, 14)
        .frame(height: 38)
        .background(.black.opacity(0.06))
    }

    @ViewBuilder
    private var leading: some View {
        if let toast = store.toast {
            HStack(spacing: 6) {
                Image(systemName: toastIcon(toast.style))
                    .foregroundStyle(toastColor(toast.style))
                Text(toast.title).font(.system(size: 12, weight: .medium))
                if let message = toast.message {
                    Text(message)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .help(toast.message ?? toast.title)
        } else if let session = store.session {
            HStack(spacing: 6) {
                IconView(icon: Icon(session.command.icon), size: 16)
                Text(session.command.title)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }
        } else {
            Text("Incant")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
        }
    }

    private func toastIcon(_ style: ToastStyle) -> String {
        switch style {
        case .success: "checkmark.circle.fill"
        case .failure: "xmark.octagon.fill"
        case .info: "info.circle.fill"
        }
    }

    private func toastColor(_ style: ToastStyle) -> Color {
        switch style {
        case .success: .green
        case .failure: .red
        case .info: .blue
        }
    }
}

struct KeyHint: View {
    let title: String
    let keys: String

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
            Text(keys)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 4))
        }
    }
}

struct ActionPanel: View {
    let store: Store

    var body: some View {
        let actions = store.currentActions
        VStack(alignment: .leading, spacing: 2) {
            if actions.isEmpty {
                Text("No actions")
                    .foregroundStyle(.secondary)
                    .padding(10)
            }
            ForEach(Array(actions.enumerated()), id: \.element.id) { index, action in
                HStack(spacing: 10) {
                    IconView(icon: Icon(action.icon ?? "circle"), size: 16)
                    Text(action.title).font(.system(size: 13))
                    Spacer(minLength: 16)
                    if index == 0 {
                        Text("↵").font(.system(size: 11)).foregroundStyle(.secondary)
                    } else if index == 1 {
                        Text("⌘↵").font(.system(size: 11)).foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 8)
                .frame(height: 30)
                .background(
                    RoundedRectangle(cornerRadius: 6)
                        .fill(index == store.actionsSelection ? Color.primary.opacity(0.1) : .clear)
                )
                .contentShape(Rectangle())
                .onTapGesture { store.perform(action) }
            }
        }
        .padding(6)
        .frame(width: 280)
        .background(.thickMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.white.opacity(0.12)))
        .shadow(color: .black.opacity(0.25), radius: 12, y: 4)
    }
}
