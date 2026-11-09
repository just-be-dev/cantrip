//! Small fuzzy matcher: prefix > word-start > substring > acronym > subsequence.

/// Higher is better; `None` means no match. An empty query matches everything with 0.
pub fn score(query: &str, text: &str) -> Option<i64> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Some(0);
    }
    let hay = text.to_lowercase();
    let len = hay.chars().count() as i64;
    if hay == needle {
        return Some(2000);
    }
    if hay.starts_with(&needle) {
        return Some(1500 - len);
    }
    if let Some(at) = hay.find(&needle) {
        let at_word_start = at == 0
            || !hay[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic);
        return Some(if at_word_start { 1200 } else { 900 } - len);
    }
    if let Some(acronym) = acronym_score(&needle, &hay) {
        return Some(acronym);
    }
    subsequence_score(&needle, &hay, len)
}

/// Best score across several fields; later fields count for less.
pub fn score_fields<S: AsRef<str>>(query: &str, fields: &[S]) -> Option<i64> {
    fields
        .iter()
        .enumerate()
        .filter_map(|(i, field)| score(query, field.as_ref()).map(|s| s - i as i64 * 300))
        .max()
}

/// "vsc" → "Visual Studio Code"
fn acronym_score(needle: &str, hay: &str) -> Option<i64> {
    let initials: String = hay
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| w.chars().next())
        .collect();
    initials.starts_with(needle).then_some(1000)
}

fn subsequence_score(needle: &str, hay: &str, len: i64) -> Option<i64> {
    let hay: Vec<char> = hay.chars().collect();
    let (mut score, mut streak, mut at) = (0i64, 0i64, 0usize);
    for c in needle.chars() {
        let found = at + hay[at..].iter().position(|&h| h == c)?;
        streak = if found == at { streak + 1 } else { 0 };
        score += 10 + streak * 5;
        at = found + 1;
    }
    Some(score - len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_exact_then_prefix_then_word_then_substring() {
        let exact = score("safari", "Safari").unwrap();
        let prefix = score("saf", "Safari").unwrap();
        let word = score("studio", "Visual Studio Code").unwrap();
        let inner = score("tudio", "Visual Studio Code").unwrap();
        assert!(
            exact > prefix && prefix > word && word > inner,
            "{exact} {prefix} {word} {inner}"
        );
    }

    #[test]
    fn matches_acronyms_and_subsequences_and_rejects_the_rest() {
        assert_eq!(score("vsc", "Visual Studio Code"), Some(1000));
        assert!(score("sfri", "Safari").is_some());
        assert_eq!(score("xyz", "Safari"), None);
        assert_eq!(score("  ", "anything"), Some(0));
    }

    #[test]
    fn later_fields_weigh_less() {
        let title = score_fields("uuid", &["UUID", "Generate"]).unwrap();
        let keyword = score_fields("uuid", &["Generate", "uuid"]).unwrap();
        assert!(title > keyword);
    }
}
