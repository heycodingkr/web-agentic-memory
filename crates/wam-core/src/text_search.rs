//! Deterministic fielded BM25 scoring for recall text queries.

use unicode_normalization::UnicodeNormalization;

const K1: f64 = 1.2;
const B: f64 = 0.75;

const FIELD_BOOSTS: [f64; 8] = [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.75, 0.5];

#[derive(Clone, Copy, Debug)]
pub(crate) struct SearchDocument<'a> {
    pub(crate) fields: [&'a str; 8],
}

/// Normalize already allowlisted keyword terms. Free-form task text must stay
/// in the client and never become a server-side retrieval query.
pub(crate) fn normalize_terms<'a>(terms: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut normalized: Vec<String> = terms.into_iter().flat_map(tokenize).collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

/// Scores documents using BM25 independently for each field, then combines the
/// boosted field scores. Field order is title, intent, keywords, task, summary,
/// site, domain, and memory kind.
pub(crate) fn bm25_scores(
    documents: &[SearchDocument<'_>],
    query_terms: &[String],
) -> Vec<(f64, u8)> {
    let tokenized: Vec<[Vec<String>; 8]> = documents
        .iter()
        .map(|document| std::array::from_fn(|index| tokenize(document.fields[index])))
        .collect();
    let mut scores = vec![(0.0, 0); documents.len()];
    if documents.is_empty() || query_terms.is_empty() {
        return scores;
    }

    for (field_index, boost) in FIELD_BOOSTS.iter().copied().enumerate() {
        let field_lengths: Vec<usize> = tokenized
            .iter()
            .map(|document| document[field_index].len())
            .collect();
        let average_length = field_lengths.iter().sum::<usize>() as f64 / documents.len() as f64;
        if average_length == 0.0 {
            continue;
        }

        for term in query_terms {
            let document_frequency = tokenized
                .iter()
                .filter(|document| document[field_index].iter().any(|value| value == term))
                .count();
            if document_frequency == 0 {
                continue;
            }
            let count = documents.len() as f64;
            let frequency = document_frequency as f64;
            let inverse_document_frequency =
                (1.0 + (count - frequency + 0.5) / (frequency + 0.5)).ln();

            for (index, document) in tokenized.iter().enumerate() {
                let field = &document[field_index];
                let term_frequency = field.iter().filter(|value| *value == term).count();
                if term_frequency == 0 {
                    continue;
                }
                let tf = term_frequency as f64;
                let length = field_lengths[index] as f64;
                let normalization = K1 * (1.0 - B + B * length / average_length);
                scores[index].0 +=
                    boost * inverse_document_frequency * (tf * (K1 + 1.0)) / (tf + normalization);
            }
        }
    }

    // `text_matches` counts distinct query terms across fields, not field hits.
    for (index, document) in tokenized.iter().enumerate() {
        scores[index].1 = query_terms
            .iter()
            .filter(|term| {
                document
                    .iter()
                    .any(|field| field.iter().any(|value| value == *term))
            })
            .count() as u8;
    }
    scores
}

fn tokenize(value: &str) -> Vec<String> {
    let normalized: String = value.nfkc().flat_map(char::to_lowercase).collect();
    let mut tokens = Vec::new();
    let mut token = String::new();
    for character in normalized.chars() {
        if character.is_alphanumeric() {
            token.push(character);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::{SearchDocument, bm25_scores, normalize_terms};

    #[test]
    fn fielded_bm25_prefers_title_and_normalizes_full_width_text() {
        let query = normalize_terms(["ＤＲＡＦＴ"]);
        let documents = [
            SearchDocument {
                fields: [
                    "draft editor",
                    "write-draft",
                    "write",
                    "compose-draft",
                    "",
                    "",
                    "",
                    "workflow",
                ],
            },
            SearchDocument {
                fields: [
                    "editor",
                    "browse-site",
                    "browse",
                    "navigate",
                    "draft",
                    "",
                    "",
                    "workflow",
                ],
            },
        ];

        let scores = bm25_scores(&documents, &query);
        assert!(scores[0].0 > scores[1].0);
        assert_eq!(scores[0].1, 1);
        assert_eq!(scores[1].1, 1);
    }

    #[test]
    fn bm25_uses_exact_tokens_deduplicates_queries_and_handles_empty_terms() {
        let query = normalize_terms(["Draft DRAFT", "ＤＲＡＦＴ"]);
        assert_eq!(query, vec!["draft"]);
        let documents = [
            SearchDocument {
                fields: ["drafting", "", "", "", "", "", "", "workflow"],
            },
            SearchDocument {
                fields: ["", "", "", "", "draft", "", "", "workflow"],
            },
        ];

        let scores = bm25_scores(&documents, &query);
        assert_eq!(scores[0], (0.0, 0));
        assert!(scores[1].0 > 0.0);
        assert_eq!(scores[1].1, 1);
        assert_eq!(bm25_scores(&documents, &[]), vec![(0.0, 0), (0.0, 0)]);
    }
}
