//! Fuzzy matching for the command palette: the query's characters must
//! appear in order; matches at word starts and in runs score higher.

/// A match: its score (higher is better) and which characters matched,
/// as char indices into the candidate, for highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub indices: Vec<usize>,
}

/// Matches `query` against `candidate`, ignoring case and spaces in the
/// query. An empty query matches everything with score 0.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<Match> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if query.is_empty() {
        return Some(Match {
            score: 0,
            indices: Vec::new(),
        });
    }
    let chars: Vec<char> = candidate.chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();

    // Greedy forward pass finds whether it matches at all...
    let mut indices = Vec::with_capacity(query.len());
    let mut qi = 0;
    for (i, c) in lower.iter().enumerate() {
        if qi < query.len() && *c == query[qi] {
            indices.push(i);
            qi += 1;
        }
    }
    if qi < query.len() {
        return None;
    }
    // ...then each matched character is pulled to a word start when one
    // follows before the next match, which reads better ("brn" in
    // "Batch rename" highlights B, r, n rather than a, r, n).
    for k in (0..indices.len()).rev() {
        let limit = indices.get(k + 1).copied().unwrap_or(lower.len());
        if let Some(better) =
            (indices[k] + 1..limit).find(|&i| lower[i] == query[k] && is_word_start(&chars, i))
            && !is_word_start(&chars, indices[k])
        {
            indices[k] = better;
        }
    }

    let mut score = 0;
    for (k, &i) in indices.iter().enumerate() {
        score += 1;
        if is_word_start(&chars, i) {
            score += 8;
        }
        if k > 0 && indices[k - 1] + 1 == i {
            score += 5;
        }
        if i == 0 {
            score += 4;
        }
    }
    // Shorter candidates and earlier first matches win ties.
    score -= indices[0] as i32 / 2;
    score -= (chars.len() as i32) / 16;
    Some(Match { score, indices })
}

fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    let cur = chars[i];
    matches!(prev, ' ' | '_' | '-' | '.' | '/' | '\\')
        || (prev.is_lowercase() && cur.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(q: &str, c: &str) -> i32 {
        fuzzy_match(q, c).map_or(i32::MIN, |m| m.score)
    }

    #[test]
    fn requires_all_characters_in_order() {
        assert!(fuzzy_match("brn", "Batch rename selection").is_some());
        assert!(fuzzy_match("nrb", "Batch rename").is_none());
        assert!(fuzzy_match("xyz", "Batch rename").is_none());
    }

    #[test]
    fn highlights_word_starts() {
        let m = fuzzy_match("brn", "Batch rename selection").unwrap();
        let picked: String = m
            .indices
            .iter()
            .map(|&i| "Batch rename selection".chars().nth(i).unwrap())
            .collect();
        assert_eq!(picked, "Brn");
        assert_eq!(m.indices[0], 0);
        assert_eq!(m.indices[1], 6, "r of rename, not of Batch's neighbourhood");
    }

    #[test]
    fn better_matches_score_higher() {
        assert!(score("ren", "Rename") > score("ren", "Open terminal here"));
        assert!(score("nf", "New folder") > score("nf", "Info"));
        assert!(score("cargo", "Cargo.toml") > score("cargo", "src/my-cargo-notes.md"));
    }

    #[test]
    fn case_and_spaces_dont_matter() {
        assert!(fuzzy_match("NEW F", "new folder").is_some());
        assert_eq!(fuzzy_match("", "anything").unwrap().score, 0);
    }

    #[test]
    fn paths_match_at_segments() {
        let m = fuzzy_match("abr", "assets/brand/").unwrap();
        assert_eq!(m.indices, vec![0, 7, 8]);
    }
}
