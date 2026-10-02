//! File-attribute schema: the canonical attribute names and accepted aliases.
//!
//! This is the single source of truth for attribute *naming* only. Whether an
//! attribute is actually supported is a per-volume question answered by
//! [`crate::core::attrs::AttrCaps`]. Kept dependency-free so both config
//! validation and the capability layer share one list instead of re-listing the
//! names (and the `read-only` alias) in every consumer.

/// Canonical (lowercase) attribute names.
pub const ATTR_READONLY: &str = "readonly";
pub const ATTR_HIDDEN: &str = "hidden";
pub const ATTR_SYSTEM: &str = "system";
pub const ATTR_ARCHIVE: &str = "archive";

/// Accepted alias for [`ATTR_READONLY`].
pub const ATTR_READONLY_ALIAS: &str = "read-only";

/// Attribute names in display/iteration order.
pub const ATTRIBUTE_NAMES: &[&str] = &[ATTR_READONLY, ATTR_HIDDEN, ATTR_SYSTEM, ATTR_ARCHIVE];

/// Input aliases, each mapped to its canonical name.
pub const ATTRIBUTE_ALIASES: &[(&str, &str)] = &[(ATTR_READONLY_ALIAS, ATTR_READONLY)];

/// Canonicalize an attribute token (optionally `[+|-]`-prefixed, any case) to
/// its canonical lowercase name, or `None` if it names no known attribute.
///
/// This is the only place that knows the accepted spellings; every consumer
/// (`--set-attributes`, `--filter-attr`, capability lookup) goes through it.
pub fn canonical_attribute_name(token: &str) -> Option<&'static str> {
    let name = token.trim().trim_start_matches(['+', '-']).trim();
    if name.is_empty() {
        return None;
    }
    ATTRIBUTE_NAMES
        .iter()
        .copied()
        .find(|canonical| canonical.eq_ignore_ascii_case(name))
        .or_else(|| {
            ATTRIBUTE_ALIASES
                .iter()
                .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
                .map(|(_, canonical)| *canonical)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_names_aliases_case_and_signs() {
        assert_eq!(canonical_attribute_name("readonly"), Some(ATTR_READONLY));
        assert_eq!(canonical_attribute_name("READONLY"), Some(ATTR_READONLY));
        assert_eq!(canonical_attribute_name("read-only"), Some(ATTR_READONLY));
        assert_eq!(canonical_attribute_name("-Hidden"), Some(ATTR_HIDDEN));
        assert_eq!(canonical_attribute_name("+system"), Some(ATTR_SYSTEM));
        assert_eq!(canonical_attribute_name(" archive "), Some(ATTR_ARCHIVE));
        assert_eq!(canonical_attribute_name("bogus"), None);
        assert_eq!(canonical_attribute_name(""), None);
        assert_eq!(canonical_attribute_name("-"), None);
    }

    #[test]
    fn every_alias_points_at_a_canonical_name() {
        for (alias, canonical) in ATTRIBUTE_ALIASES {
            assert_ne!(alias, canonical);
            assert!(ATTRIBUTE_NAMES.contains(canonical));
        }
    }
}
