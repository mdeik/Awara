use std::collections::HashSet;

/// A match result from searching the [`FuzzyIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub index: usize,
    pub score: u32,
}

/// In-memory trigram index for fast fuzzy searching across large file lists.
#[derive(Default, Debug, Clone)]
pub struct FuzzyIndex {
    items: Vec<String>,
    trigrams: Vec<HashSet<u32>>,
}

impl FuzzyIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a [`FuzzyIndex`] from a list of filenames.
    pub fn from_items(items: &[String]) -> Self {
        let mut index = Self {
            items: items.to_vec(),
            trigrams: Vec::with_capacity(items.len()),
        };
        for item in items {
            index.trigrams.push(extract_trigrams(&item.to_lowercase()));
        }
        index
    }

    /// Perform a fuzzy search against indexed items and return ranked matches.
    pub fn search(&self, query: &str) -> Vec<FuzzyMatch> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return (0..self.items.len())
                .map(|index| FuzzyMatch { index, score: 0 })
                .collect();
        }

        let query_trigrams = extract_trigrams(&q);
        let mut matches = Vec::new();

        for (idx, text) in self.items.iter().enumerate() {
            let lower_text = text.to_lowercase();
            // Fast exact substring match bonus
            if lower_text.contains(&q) {
                let score = 1000 + (100 / (lower_text.len().max(1) as u32));
                matches.push(FuzzyMatch { index: idx, score });
                continue;
            }

            // Trigram overlap calculation
            let item_trigrams = &self.trigrams[idx];
            if item_trigrams.is_empty() || query_trigrams.is_empty() {
                continue;
            }

            let common = query_trigrams.intersection(item_trigrams).count();
            if common > 0 {
                let score = (common * 100) as u32 / query_trigrams.len() as u32;
                matches.push(FuzzyMatch { index: idx, score });
            }
        }

        matches.sort_by_key(|b| std::cmp::Reverse(b.score));
        matches
    }

    /// Returns total indexed items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Checks if index is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Helper function to convert a string into a set of 24-bit encoded trigrams.
fn extract_trigrams(text: &str) -> HashSet<u32> {
    let chars: Vec<char> = text.chars().collect();
    let mut trigrams = HashSet::new();

    if chars.len() < 3 {
        if !chars.is_empty() {
            let mut val = 0u32;
            for &c in &chars {
                val = (val << 8) | ((c as u32) & 0xFF);
            }
            trigrams.insert(val);
        }
        return trigrams;
    }

    for window in chars.windows(3) {
        let tri = ((window[0] as u32 & 0xFF) << 16)
            | ((window[1] as u32 & 0xFF) << 8)
            | (window[2] as u32 & 0xFF);
        trigrams.insert(tri);
    }
    trigrams
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_index_exact_substring() {
        let files = vec![
            "invoice_2026_01.pdf".to_string(),
            "report_final.docx".to_string(),
            "invoice_summary.xlsx".to_string(),
        ];
        let index = FuzzyIndex::from_items(&files);

        let matches = index.search("invoice");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].index, 0); // invoice_2026_01.pdf
        assert_eq!(matches[1].index, 2); // invoice_summary.xlsx
    }

    #[test]
    fn test_fuzzy_index_empty_query() {
        let files = vec!["a.txt".to_string(), "b.txt".to_string()];
        let index = FuzzyIndex::from_items(&files);

        let matches = index.search("");
        assert_eq!(matches.len(), 2);
    }
}
