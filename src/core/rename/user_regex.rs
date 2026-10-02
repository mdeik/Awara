//! Single source of truth for compiling user-supplied regular expressions.
//!
//! Every user regex field — the RegEx section, the filter Mask in regex mode,
//! and Exclude — is compiled through [`build_regex`], which defines the
//! defaults contract:
//!
//!   * matching is **case-sensitive** (opt in with an inline `(?i)`);
//!   * whitespace is **literal** (opt in to verbose mode with an inline `(?x)`);
//!   * the pattern is never rewritten or relaxed behind the user's back.
//!
//! Keeping this in one place means the three call sites cannot drift. How a
//! *failure* is handled is deliberately left to the caller: the rename pipeline
//! skips an uncompilable pattern, while Mask/Exclude fail open (never filter).

use regex::{Regex, RegexBuilder};
use std::borrow::Cow;

/// Compile a user regex using the canonical defaults (see module docs).
pub(crate) fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
    Regex::new(pattern)
}

/// The effective pattern for the RegEx section. Simple mode escapes every regex
/// metacharacter so the pattern is matched literally; otherwise the pattern is
/// used verbatim. Shared by preview/compile and execution so both agree.
pub(crate) fn effective_pattern(simple: bool, pattern: &str) -> Cow<'_, str> {
    if simple {
        Cow::Owned(regex::escape(pattern))
    } else {
        Cow::Borrowed(pattern)
    }
}

/// Case-aware `\b<word>\b` matcher used by Remove Words.
///
/// Remove Words is not a regex field, so it keeps its own
/// `--replace-case-sensitive` control rather than the case-sensitive default
/// above.
pub(crate) fn build_word_regex(word: &str, case_sensitive: bool) -> Result<Regex, regex::Error> {
    let pat = format!(r"\b{}\b", regex::escape(word));
    RegexBuilder::new(&pat)
        .case_insensitive(!case_sensitive)
        .build()
}
