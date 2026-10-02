//! Identity of the config sections: the single source of truth for section
//! labels and their default execution order.
//!
//! The GUI uses this enum for layout and enable flags, the core uses it to
//! expand a section-based [`RenameConfig`](crate::RenameConfig) into
//! `RenameItem`s (see `emit_section_items`), and `section_order` persists the
//! [`label`](SectionId::label) strings. Keeping the enum here means the label
//! set, the execution order, and the config fields can never drift apart.

/// One config section, in discriminant order.
///
/// The discriminant is used as a stable index into the GUI's
/// fixed-size `SectionEnabled` array.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SectionId {
    Regex,
    Replace,
    Remove,
    Add,
    Numbering,
    Case,
    Extension,
    Name,
    AutoDate,
    AppendFolder,
    MoveCopy,
    Filters,
    CopyTo,
    NameSegment,
    Special,
}

impl SectionId {
    /// Every variant, in discriminant order. Add new variants here too.
    pub const VALUES: [SectionId; 15] = [
        SectionId::Regex,
        SectionId::Replace,
        SectionId::Remove,
        SectionId::Add,
        SectionId::Numbering,
        SectionId::Case,
        SectionId::Extension,
        SectionId::Name,
        SectionId::AutoDate,
        SectionId::AppendFolder,
        SectionId::MoveCopy,
        SectionId::Filters,
        SectionId::CopyTo,
        SectionId::NameSegment,
        SectionId::Special,
    ];

    /// Derived from [`VALUES`](SectionId::VALUES) — no manual count to maintain.
    pub const COUNT: usize = Self::VALUES.len();

    /// The order sections run in when the config has no explicit
    /// `section_order`/`command_order`. This is the sole default order; the GUI
    /// and [`config_to_items`](crate::config_to_items) both derive from it.
    ///
    /// Filters, Copy/Move to Location, and Special are intentionally absent:
    /// they produce no `RenameItem`s.
    pub const DEFAULT_ORDER: [SectionId; 12] = [
        SectionId::NameSegment,
        SectionId::Regex,
        SectionId::Extension,
        SectionId::Replace,
        SectionId::Name,
        SectionId::Remove,
        SectionId::MoveCopy,
        SectionId::Add,
        SectionId::Numbering,
        SectionId::AppendFolder,
        SectionId::Case,
        SectionId::AutoDate,
    ];

    /// The default execution order (see [`DEFAULT_ORDER`](SectionId::DEFAULT_ORDER)).
    #[inline]
    pub fn default_order() -> &'static [SectionId] {
        &Self::DEFAULT_ORDER
    }

    /// Safe zero-cost conversion from a `VALUES` index to `SectionId`.
    /// Returns `None` if out of range.
    #[inline]
    pub fn from_usize(i: usize) -> Option<SectionId> {
        Self::VALUES.get(i).copied()
    }

    /// Zero-cost conversion to the `usize` index (the enum discriminant).
    #[inline]
    pub fn as_usize(self) -> usize {
        self as usize
    }

    /// Look up a `SectionId` by its [`label`](SectionId::label).
    /// Returns `None` if the label doesn't match any variant.
    #[inline]
    pub fn from_label(label: &str) -> Option<SectionId> {
        Self::VALUES.iter().find(|id| id.label() == label).copied()
    }

    pub fn label(self) -> &'static str {
        match self {
            SectionId::Regex => "RegEx",
            SectionId::Replace => "Replace",
            SectionId::Remove => "Remove",
            SectionId::Add => "Add",
            SectionId::Numbering => "Numbering",
            SectionId::Case => "Case",
            SectionId::Extension => "Extension",
            SectionId::Name => "Name",
            SectionId::AutoDate => "Auto Date",
            SectionId::AppendFolder => "Append Folder Name",
            SectionId::MoveCopy => "Move / Copy Parts",
            SectionId::Filters => "Filters",
            SectionId::CopyTo => "Copy / Move to Location",
            SectionId::NameSegment => "Name Segment",
            SectionId::Special => "Special",
        }
    }
}

/// Per-section enable flags, indexed by [`SectionId`]. A runtime view of the
/// section gating stored in [`RenameDoc`](crate::RenameDoc) as a disabled list.
pub type SectionEnabled = [bool; SectionId::COUNT];

/// Default enable state: every section on.
pub const DEFAULT_SECTION_ENABLED: SectionEnabled = [true; SectionId::COUNT];

/// Build an enable array from a disabled-section list.
pub fn enabled_from_disabled(disabled: &[SectionId]) -> SectionEnabled {
    let mut enabled = DEFAULT_SECTION_ENABLED;
    for id in disabled {
        enabled[id.as_usize()] = false;
    }
    enabled
}

/// The disabled sections implied by an enable array (SSoT conversion).
pub fn disabled_from_enabled(enabled: &SectionEnabled) -> Vec<SectionId> {
    SectionId::VALUES
        .iter()
        .copied()
        .filter(|id| !enabled[id.as_usize()])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_indexable_by_discriminant() {
        for (i, id) in SectionId::VALUES.iter().enumerate() {
            assert_eq!(id.as_usize(), i, "VALUES order must match discriminants");
            assert_eq!(SectionId::from_usize(i), Some(*id));
        }
        assert_eq!(SectionId::from_usize(SectionId::COUNT), None);
    }

    #[test]
    fn labels_are_unique_and_round_trip() {
        let mut labels: Vec<&str> = SectionId::VALUES.iter().map(|s| s.label()).collect();
        let before = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), before, "section labels must be unique");

        for id in SectionId::VALUES {
            assert_eq!(SectionId::from_label(id.label()), Some(id));
        }
    }

    #[test]
    fn default_order_is_drawn_from_values_without_duplicates() {
        let mut seen = Vec::new();
        for id in SectionId::default_order() {
            assert!(SectionId::VALUES.contains(id), "{id:?} not in VALUES");
            assert!(!seen.contains(id), "{id:?} appears twice in default order");
            seen.push(*id);
        }
    }
}
