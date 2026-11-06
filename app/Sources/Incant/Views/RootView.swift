import SwiftUI

struct RootView: View {
    @Bindable var store: Store
    @FocusState private var searchFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            searchBar
            LoadingBar(active: store.session?.loading ?? false)
            Divider().opacity(0.5)
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider().opacity(0.5)
            FooterView(store: store)
        }
        .background(.regularMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(.white.opacity(0.12), lineWidth: 1)
        )
        .overlay(alignment: .bottomTrailing) {
            if store.actionsOpen {
                ActionPanel(store: store)
                    .padding(.trailing, 10)
                    .padding(.bottom, 46)
                    .transition(.opacity.combined(with: .scale(scale: 0.97, anchor: .bottomTrailing)))
            }
        }
        .animation(.easeOut(duration: 0.12), value: store.actionsOpen)
        .onAppear { searchFocused = true }
        .onChange(of: store.focusToken) { searchFocused = true }
    }

    private var searchBar: some View {
        HStack(spacing: 10) {
            if let session = store.session {
                Button {
                    store.popFrame(session)
                } label: {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 13, weight: .semibold))
                        .frame(width: 24, height: 24)
                        .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
                }
                .buttonStyle(.plain)
            } else {
                Image(systemName: "wand.and.sparkles")
                    .font(.system(size: 18, weight: .medium))
                    .foregroundStyle(.tint)
                    .frame(width: 24, height: 24)
            }
            TextField(store.placeholder, text: $store.query)
                .textFieldStyle(.plain)
                .font(.system(size: 20, weight: .regular))
                .focused($searchFocused)
                // Real ↵ is caught by the key monitor; this covers accessibility (AXConfirm).
                .onSubmit { store.submit() }
        }
        .padding(.horizontal, 16)
        .frame(height: 56)
    }

    @ViewBuilder
    private var content: some View {
        if let session = store.session, let error = session.error {
            ErrorView(title: error.title, message: error.message)
        } else if case let .detail(markdown, metadata, _)? = store.currentView {
            DetailView(markdown: markdown, metadata: metadata)
        } else if store.session != nil, store.currentView == nil {
            Color.clear
        } else {
            ResultsList(store: store)
        }
    }
}

/// A thin animated bar under the search field while a spell is working.
struct LoadingBar: View {
    let active: Bool
    @State private var phase: CGFloat = -0.3

    var body: some View {
        GeometryReader { geo in
            if active {
                Rectangle()
                    .fill(LinearGradient(colors: [.clear, .accentColor, .clear], startPoint: .leading, endPoint: .trailing))
                    .frame(width: geo.size.width * 0.3)
                    .offset(x: geo.size.width * phase)
                    .onAppear {
                        phase = -0.3
                        withAnimation(.linear(duration: 1).repeatForever(autoreverses: false)) { phase = 1 }
                    }
            }
        }
        .frame(height: 2)
        .clipped()
    }
}

struct ErrorView: View {
    let title: String
    let message: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                Label(title, systemImage: "exclamationmark.triangle.fill")
                    .font(.headline)
                    .foregroundStyle(.red)
                Text(message)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(20)
        }
    }
}
