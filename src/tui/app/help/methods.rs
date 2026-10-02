use super::*;

impl App {
    pub fn help_filter_name(&self) -> &'static str {
        if self.help_filter_index == 0 {
            "All"
        } else {
            self.help_filter_categories[self.help_filter_index - 1]
        }
    }

    pub fn help_entry_matches_filter(&self, entry_index: usize) -> bool {
        if self.help_filter_index == 0 {
            return true; // All
        }
        if let Some(cat) = self.help_filter_categories.get(self.help_filter_index - 1)
            && let Some(entry) = self.help_entries.get(entry_index)
        {
            return entry.category == *cat;
        }
        false
    }

    pub fn help_cycle_filter(&mut self) {
        let total = self.help_filter_categories.len() + 1; // +1 for "All"
        self.help_filter_index = (self.help_filter_index + 1) % total;
        // Selection is a position in the filtered view; start at the top.
        self.help_state.select(Some(0));
    }

    /// Entry indices currently visible in the help list, derived from the
    /// active filter. `help_filter_index` + `help_entries` are the source of
    /// truth; this is rebuilt every frame in draw() so it can never drift.
    pub fn compute_help_visible(&self) -> Vec<usize> {
        self.help_entries
            .iter()
            .enumerate()
            .filter(|(i, _)| self.help_entry_matches_filter(*i))
            .map(|(i, _)| i)
            .collect()
    }

    /// The HelpEntry under the current selection in the filtered view, if any.
    pub fn help_selected_entry(&self) -> Option<&HelpEntry> {
        let pos = self.help_state.selected()?;
        let entry_index = *self.help_visible.get(pos)?;
        self.help_entries.get(entry_index)
    }
}
