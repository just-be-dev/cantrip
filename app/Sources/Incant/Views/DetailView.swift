import SwiftUI

struct DetailView: View {
    let markdown: String
    let metadata: [WireMetadata]

    var body: some View {
        HStack(spacing: 0) {
            ScrollView {
                MarkdownView(markdown: markdown)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
            }
            if !metadata.isEmpty {
                Divider().opacity(0.5)
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(metadata, id: \.self) { item in
                            VStack(alignment: .leading, spacing: 3) {
                                Text(item.label)
                                    .font(.system(size: 11, weight: .semibold))
                                    .foregroundStyle(.secondary)
                                Text(item.value)
                                    .font(.system(size: 13))
                                    .textSelection(.enabled)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(16)
                }
                .frame(width: 220)
            }
        }
    }
}

/// Block-level markdown: headings, lists, quotes, fenced code, and paragraphs.
/// Inline styling (bold, code, links) comes from AttributedString.
struct MarkdownView: View {
    let markdown: String

    private enum Block: Hashable {
        case heading(level: Int, text: String)
        case bullet(String)
        case quote(String)
        case code(String)
        case paragraph(String)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case let .heading(level, text):
                    inline(text).font(.system(size: [26, 20, 16, 14][min(level, 4) - 1], weight: .bold))
                case let .bullet(text):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text("•").foregroundStyle(.secondary)
                        inline(text)
                    }
                case let .quote(text):
                    inline(text)
                        .foregroundStyle(.secondary)
                        .padding(.leading, 10)
                        .overlay(alignment: .leading) { Rectangle().fill(.tertiary).frame(width: 3) }
                case let .code(text):
                    Text(text)
                        .font(.system(size: 12, design: .monospaced))
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(.quaternary.opacity(0.6), in: RoundedRectangle(cornerRadius: 6))
                case let .paragraph(text):
                    inline(text)
                }
            }
        }
        .textSelection(.enabled)
        .font(.system(size: 14))
    }

    private func inline(_ text: String) -> Text {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        var attributed = (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
        for run in attributed.runs where run.inlinePresentationIntent?.contains(.code) == true {
            attributed[run.range].font = .system(size: 13, design: .monospaced)
            attributed[run.range].backgroundColor = Color.primary.opacity(0.08)
        }
        return Text(attributed)
    }

    private var blocks: [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var code: [String]?

        func flush() {
            if !paragraph.isEmpty { blocks.append(.paragraph(paragraph.joined(separator: " "))) }
            paragraph = []
        }

        for line in markdown.components(separatedBy: "\n") {
            if line.hasPrefix("```") {
                if let lines = code {
                    blocks.append(.code(lines.joined(separator: "\n")))
                    code = nil
                } else {
                    flush()
                    code = []
                }
                continue
            }
            if code != nil {
                code?.append(line)
                continue
            }
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty {
                flush()
            } else if let hashes = trimmed.prefix(while: { $0 == "#" }).count as Int?, hashes > 0, hashes <= 6,
                trimmed.dropFirst(hashes).hasPrefix(" ")
            {
                flush()
                blocks.append(.heading(level: hashes, text: String(trimmed.dropFirst(hashes + 1))))
            } else if trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") {
                flush()
                blocks.append(.bullet(String(trimmed.dropFirst(2))))
            } else if trimmed.hasPrefix("> ") {
                flush()
                blocks.append(.quote(String(trimmed.dropFirst(2))))
            } else {
                paragraph.append(trimmed)
            }
        }
        if let lines = code { blocks.append(.code(lines.joined(separator: "\n"))) }
        flush()
        return blocks
    }
}
