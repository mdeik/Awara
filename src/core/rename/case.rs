use super::*;

pub(crate) fn apply_case(s: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Title => {
            let mut result = String::new();
            let mut capitalize_next = true;
            for c in s.chars() {
                if capitalize_next {
                    result.push_str(&c.to_uppercase().to_string());
                    capitalize_next = false;
                } else {
                    result.push_str(&c.to_lowercase().to_string());
                }
                if !c.is_alphanumeric() {
                    capitalize_next = true;
                }
            }
            result
        }
        CaseMode::TitleEnhanced => {
            // Title Enhanced: capitalize all words, but skip short articles/prepositions
            // unless they are at the start or end of the string.
            let lowercase_words = [
                "a", "an", "and", "as", "at", "but", "by", "en", "for", "if", "in", "nor", "of",
                "on", "or", "per", "the", "to", "v", "vs", "via",
            ];
            let words: Vec<&str> = s.split_inclusive(|c: char| !c.is_alphanumeric()).collect();
            let mut result = String::new();
            for (i, w) in words.iter().enumerate() {
                if w.is_empty() {
                    continue;
                }
                // Split off trailing non-alphanumeric chars
                let alnum_end = w.trim_end_matches(|c: char| !c.is_alphanumeric());
                let trailing: String = w[alnum_end.len()..].to_string();
                if alnum_end.is_empty() {
                    result.push_str(&trailing);
                    continue;
                }
                let lower = alnum_end.to_lowercase();
                let is_first = i == 0;
                let is_last = i == words.len() - 1;
                if !is_first && !is_last && lowercase_words.contains(&lower.as_str()) {
                    result.push_str(&lower);
                } else {
                    let mut chars = alnum_end.chars();
                    if let Some(first) = chars.next() {
                        result.push_str(&first.to_uppercase().to_string());
                        result.push_str(&chars.as_str().to_lowercase());
                    }
                }
                result.push_str(&trailing);
            }
            result
        }
        CaseMode::Sentence => {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => {
                    f.to_uppercase().collect::<String>() + c.as_str().to_lowercase().as_str()
                }
            }
        }
        CaseMode::Invert => s
            .chars()
            .map(|c| {
                if c.is_lowercase() {
                    c.to_uppercase().next().unwrap()
                } else {
                    c.to_lowercase().next().unwrap()
                }
            })
            .collect(),
    }
}

pub(crate) fn apply_case_exceptions(s: &str, exceptions: &str) -> String {
    let mut result = s.to_string();
    for word in exceptions.split([' ', ',']).filter(|w| !w.is_empty()) {
        if let Ok(re) = RegexBuilder::new(&regex::escape(word))
            .case_insensitive(true)
            .build()
        {
            result = re.replace_all(&result, word).to_string();
        }
    }
    result
}
