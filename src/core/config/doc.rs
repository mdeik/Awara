//! The shared rename document: a [`RenameConfig`] plus the set of disabled
//! sections.
//!
//! This is the single on-disk format for presets and saved state across the GUI,
//! CLI, and TUI. Disabled sections are honored everywhere: every consumer clears
//! them to their defaults via [`RenameDoc::effective_config`] before preview or
//! execution. Only the GUI authors disabled sections; the CLI and TUI always
//! write all-enabled (see [`RenameDoc::new`]).

use super::*;
use serde::Deserialize;
use std::borrow::Cow;
use std::path::Path;

/// A rename configuration together with the sections that are switched off.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RenameDoc {
    /// The rename configuration.
    pub rename_config: RenameConfig,
    /// Sections that are switched off, serialized by name (e.g. `["AutoDate"]`).
    /// Empty or absent means every section is enabled. Unknown names are ignored
    /// on load, so a file written by a newer version still loads here.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "deserialize_disabled_sections"
    )]
    pub disabled_sections: Vec<SectionId>,
}

impl Default for RenameDoc {
    fn default() -> Self {
        Self::new(RenameConfig::default())
    }
}

/// A parsed document plus the non-fatal cleanups applied while loading it.
pub struct LoadedDoc {
    pub doc: RenameDoc,
    /// Human-readable messages for values that were cleaned/normalized on load.
    pub warnings: Vec<String>,
}

impl RenameDoc {
    /// A document with every section enabled — the form CLI/TUI produce.
    pub fn new(rename_config: RenameConfig) -> Self {
        Self {
            rename_config: rename_config.cleaned(),
            disabled_sections: Vec::new(),
        }
    }

    /// A document from an explicit config + disabled sections (GUI authoring).
    pub fn with_disabled(rename_config: RenameConfig, disabled_sections: Vec<SectionId>) -> Self {
        Self {
            rename_config: rename_config.cleaned(),
            disabled_sections: normalize_disabled(disabled_sections),
        }
    }

    /// Canonical form: cleaned config + normalized (sorted, deduped) disabled list.
    pub fn cleaned(mut self) -> Self {
        self.rename_config = self.rename_config.cleaned();
        self.disabled_sections = normalize_disabled(self.disabled_sections);
        self
    }

    /// The enable array (GUI runtime view) implied by [`disabled_sections`].
    ///
    /// [`disabled_sections`]: RenameDoc::disabled_sections
    pub fn enabled_sections(&self) -> SectionEnabled {
        enabled_from_disabled(&self.disabled_sections)
    }

    /// The config as it should be previewed/executed, with disabled sections
    /// cleared to their defaults. Borrows the inner config when nothing is
    /// disabled (the common case).
    pub fn effective_config(&self) -> Cow<'_, RenameConfig> {
        if self.disabled_sections.is_empty() {
            return Cow::Borrowed(&self.rename_config);
        }
        cleared_config(&self.rename_config, &self.enabled_sections())
    }

    /// Parse and sanitize a document, discarding cleanup warnings. Only the
    /// enveloped form is accepted; a bare `RenameConfig` object is rejected
    /// (there is no legacy support). Use [`Self::from_json_with_warnings`] to
    /// surface what was cleaned.
    pub fn from_json(data: &str) -> Result<Self, String> {
        Self::from_json_with_warnings(data).map(|loaded| loaded.doc)
    }

    /// Parse and sanitize a document, also returning the cleanup warnings
    /// produced by [`RenameConfig::sanitize`]. Structural problems are errors.
    pub fn from_json_with_warnings(data: &str) -> Result<LoadedDoc, String> {
        let mut doc: Self = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let warnings = doc.rename_config.sanitize();
        Ok(LoadedDoc { doc, warnings })
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.clone().cleaned()).map_err(|e| e.to_string())
    }
}

/// Read, parse, and sanitize a rename document from disk (warnings discarded).
pub fn load_rename_doc(path: &Path) -> Result<RenameDoc, String> {
    load_rename_doc_with_warnings(path).map(|loaded| loaded.doc)
}

/// Read, parse, and sanitize a rename document from disk, returning the cleanups.
pub fn load_rename_doc_with_warnings(path: &Path) -> Result<LoadedDoc, String> {
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    RenameDoc::from_json_with_warnings(&data)
}

/// Sort and dedupe a disabled list so serialization is deterministic and
/// order-independent.
fn normalize_disabled(mut disabled: Vec<SectionId>) -> Vec<SectionId> {
    disabled.sort_by_key(|s| s.as_usize());
    disabled.dedup();
    disabled
}

/// Lenient deserializer for [`RenameDoc::disabled_sections`]: unknown section
/// names are dropped rather than failing the whole document. This keeps the
/// format forward-compatible — a section added in a later release, and disabled
/// in a file, does not make that file unreadable by older builds.
fn deserialize_disabled_sections<'de, D>(deserializer: D) -> Result<Vec<SectionId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value::<SectionId>(v).ok())
        .collect())
}

/// SSoT for section gating: clear disabled sections to their defaults. Returns
/// the input borrowed, unchanged, when every section is enabled.
pub fn cleared_config<'a>(
    cfg: &'a RenameConfig,
    section_enabled: &SectionEnabled,
) -> Cow<'a, RenameConfig> {
    if section_enabled.iter().all(|&enabled| enabled) {
        return Cow::Borrowed(cfg);
    }
    let mut working = cfg.clone();
    for (i, &enabled) in section_enabled.iter().enumerate().take(SectionId::COUNT) {
        if !enabled && let Some(id) = SectionId::from_usize(i) {
            working.clear_section(id);
        }
    }
    Cow::Owned(working)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_sanitizes() {
        let doc = RenameDoc {
            rename_config: RenameConfig {
                numbering: NumberingSection {
                    numbering_type: Some("hexadecimal".into()),
                    ..Default::default()
                },
                ..Default::default()
            },
            disabled_sections: Vec::new(),
        };
        let json = doc.to_json_pretty().unwrap();
        let back = RenameDoc::from_json(&json).unwrap();
        // Invalid semantics are cleaned, not rejected.
        assert_eq!(back.rename_config.numbering.numbering_type, None);
        assert!(back.disabled_sections.is_empty());
    }

    #[test]
    fn bare_config_is_rejected() {
        // No legacy support: the enveloped form is required.
        assert!(RenameDoc::from_json(r#"{"replace": {"replace": "a"}}"#).is_err());
        assert!(RenameDoc::from_json("not json").is_err());
    }

    #[test]
    fn missing_disabled_sections_defaults_to_all_enabled() {
        let doc = RenameDoc::from_json(r#"{"rename_config": {}}"#).unwrap();
        assert!(doc.disabled_sections.is_empty());
        assert_eq!(doc.enabled_sections(), DEFAULT_SECTION_ENABLED);
    }

    #[test]
    fn disabled_sections_serialize_by_name() {
        let doc = RenameDoc::with_disabled(RenameConfig::default(), vec![SectionId::AutoDate]);
        let json = doc.to_json_pretty().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["disabled_sections"], serde_json::json!(["AutoDate"]));
    }

    #[test]
    fn unknown_disabled_section_names_are_ignored() {
        let doc = RenameDoc::from_json(
            r#"{"rename_config":{},"disabled_sections":["AutoDate","NotASection"]}"#,
        )
        .unwrap();
        assert_eq!(doc.disabled_sections, vec![SectionId::AutoDate]);
    }

    #[test]
    fn preset_values_are_clamped_to_limits() {
        let doc =
            RenameDoc::from_json(r#"{"rename_config":{"append_folder":{"dirname_level":9999}}}"#)
                .unwrap();
        assert_eq!(
            doc.rename_config.append_folder.dirname_level,
            DIRNAME_LEVEL.1
        );
    }

    #[test]
    fn loads_warnings_for_cleaned_values() {
        let loaded = RenameDoc::from_json_with_warnings(
            r#"{"rename_config":{"append_folder":{"dirname_level":9999}}}"#,
        )
        .unwrap();
        assert_eq!(
            loaded.doc.rename_config.append_folder.dirname_level,
            DIRNAME_LEVEL.1
        );
        assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
    }

    #[test]
    fn effective_config_clears_only_disabled_sections() {
        let mut cfg = RenameConfig::default();
        cfg.replace.replace = Some("keep".into());
        cfg.add.add_prefix = Some("drop".into());
        let doc = RenameDoc::with_disabled(cfg, vec![SectionId::Add]);
        let effective = doc.effective_config();
        assert_eq!(effective.replace.replace.as_deref(), Some("keep"));
        assert_eq!(effective.add.add_prefix, None, "disabled section cleared");
    }

    #[test]
    fn effective_config_borrows_when_all_enabled() {
        let doc = RenameDoc::new(RenameConfig::default());
        assert!(matches!(doc.effective_config(), Cow::Borrowed(_)));
    }
}
