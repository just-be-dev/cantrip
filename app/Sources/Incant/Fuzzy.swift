// Small fuzzy matcher: prefix > word-start > substring > subsequence.

import Foundation

enum Fuzzy {
    /// Higher is better; nil means no match. An empty query matches everything with 0.
    static func score(_ query: String, _ text: String) -> Int? {
        let needle = query.lowercased().trimmingCharacters(in: .whitespaces)
        if needle.isEmpty { return 0 }
        let hay = text.lowercased()
        if hay == needle { return 2000 }
        if hay.hasPrefix(needle) { return 1500 - hay.count }
        if let range = hay.range(of: needle) {
            let atWordStart = range.lowerBound == hay.startIndex
                || !hay[hay.index(before: range.lowerBound)].isLetter
            return (atWordStart ? 1200 : 900) - hay.count
        }
        if let acronym = acronymScore(needle, hay) { return acronym }
        return subsequenceScore(needle, hay)
    }

    /// Best score across several fields; later fields count for less.
    static func score(_ query: String, fields: [String]) -> Int? {
        var best: Int?
        for (index, field) in fields.enumerated() {
            guard let fieldScore = score(query, field) else { continue }
            let weighted = fieldScore - index * 300
            best = max(best ?? weighted, weighted)
        }
        return best
    }

    /// "vsc" → "Visual Studio Code"
    private static func acronymScore(_ needle: String, _ hay: String) -> Int? {
        let initials = hay.split(whereSeparator: { !$0.isLetter && !$0.isNumber }).compactMap(\.first)
        return String(initials).hasPrefix(needle) ? 1000 : nil
    }

    private static func subsequenceScore(_ needle: String, _ hay: String) -> Int? {
        var score = 0
        var streak = 0
        var hayIndex = hay.startIndex
        for char in needle {
            guard let found = hay[hayIndex...].firstIndex(of: char) else { return nil }
            streak = found == hayIndex ? streak + 1 : 0
            score += 10 + streak * 5
            hayIndex = hay.index(after: found)
        }
        return score - hay.count
    }
}
