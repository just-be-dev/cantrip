import SwiftUI

struct ResultsList: View {
    @Bindable var store: Store

    var body: some View {
        let rows = store.rows
        if rows.isEmpty {
            VStack(spacing: 8) {
                Image(systemName: "sparkle.magnifyingglass")
                    .font(.system(size: 28))
                    .foregroundStyle(.tertiary)
                Text(emptyText)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 2) {
                        if store.session == nil, store.query.isEmpty {
                            SectionHeader(title: "Results")
                        }
                        ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                            RowView(row: row, selected: index == store.selection)
                                .id(row.id)
                                .contentShape(Rectangle())
                                .onTapGesture {
                                    store.selection = index
                                    if let first = row.actions.first { store.perform(first) }
                                }
                        }
                    }
                    .padding(8)
                }
                .scrollIndicators(.never)
                .onChange(of: store.selection) {
                    if rows.indices.contains(store.selection) {
                        proxy.scrollTo(rows[store.selection].id)
                    }
                }
            }
        }
    }

    private var emptyText: String {
        if case let .list(_, _, emptyText, _)? = store.currentView, let emptyText, store.query.isEmpty {
            return emptyText
        }
        return store.query.isEmpty ? "Nothing here" : "No results for “\(store.query)”"
    }
}

struct SectionHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.top, 2)
            .padding(.bottom, 4)
    }
}

struct RowView: View {
    let row: Row
    let selected: Bool

    var body: some View {
        HStack(spacing: 12) {
            IconView(icon: row.icon, size: 22)
            Text(row.title)
                .font(.system(size: 14))
                .lineLimit(1)
            if let subtitle = row.subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            ForEach(row.accessories, id: \.self) { accessory in
                Text(accessory)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 40)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(selected ? Color.primary.opacity(0.1) : .clear)
        )
    }
}
