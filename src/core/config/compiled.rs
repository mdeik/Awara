use super::*;

#[derive(Clone)]
pub struct CompiledConfig {
    pub config: RenameConfig,
    pub re_match: Option<Regex>,
    pub re_words: Vec<Regex>,
    pub re_double_space: Option<Regex>,
    /// Date-detection patterns used by `NameReformatDate`. Compiled once per
    /// config instead of on every filename.
    pub re_yyyy_mm_dd: Option<Regex>,
    pub re_mm_dd_yyyy: Option<Regex>,
    /// Whether the pipeline must read EXIF dates (see [`config_needs_exif_date`]).
    /// Hoisted here so the per-file path does not rescan `command_order`, and so
    /// EXIF is only read for configs that actually consume it.
    pub needs_exif_date: bool,
}

impl CompiledConfig {
    pub fn new(config: RenameConfig) -> Self {
        let re_match = if let Some(pattern) = &config.regex.regex_match {
            let pattern =
                crate::core::rename::effective_pattern(config.regex.regex_simple, pattern);
            crate::core::rename::build_regex(&pattern).ok()
        } else {
            None
        };

        let re_words = if let Some(words) = &config.remove.remove_words {
            words
                .split_whitespace()
                .filter_map(|w| {
                    crate::core::rename::build_word_regex(w, config.replace.replace_case_sensitive)
                        .ok()
                })
                .collect()
        } else {
            Vec::new()
        };

        let re_double_space = if config.remove.double_spaces {
            Regex::new(r"\s+").ok()
        } else {
            None
        };

        // Date detection for NameReformatDate — hoisted out of the per-file
        // pipeline so it is compiled once per config.
        let re_yyyy_mm_dd = Regex::new(r"(\d{4})[-_.](\d{2})[-_.](\d{2})").ok();
        let re_mm_dd_yyyy = Regex::new(r"(\d{2})[-_.](\d{2})[-_.](\d{4})").ok();

        // SSoT: when command_order is empty, translate section-based config into items
        let mut config = config;
        if config.command_order.is_empty() {
            config.command_order = super::config_to_items(&config);
        }

        // After the translation above, so it sees the items that will really run.
        let needs_exif_date = super::config_needs_exif_date(&config);

        CompiledConfig {
            config,
            re_match,
            re_words,
            re_double_space,
            re_yyyy_mm_dd,
            re_mm_dd_yyyy,
            needs_exif_date,
        }
    }

    /// Create from a reference (clones internally).
    pub fn from_ref(config: &RenameConfig) -> Self {
        Self::new(config.clone())
    }
}
