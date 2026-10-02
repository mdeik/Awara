use super::*;

impl GuiApp {
    pub fn move_selected_up(&mut self) {
        let order = self.get_display_order();
        if order.is_empty() {
            return;
        }
        let sel_positions: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|&(_, &idx)| self.selection.get(idx).copied().unwrap_or(false))
            .map(|(pos, _)| pos)
            .collect();
        if sel_positions.is_empty() || sel_positions[0] == 0 {
            return;
        }
        let sel_set: HashSet<usize> = sel_positions.iter().copied().collect();
        let first_sel = sel_positions[0];
        let mut block_end = first_sel;
        while block_end + 1 < order.len() && sel_set.contains(&(block_end + 1)) {
            block_end += 1;
        }
        let mut new_order = (*order).clone();
        let item_above = new_order.remove(first_sel - 1);
        new_order.insert(block_end, item_above);
        self.custom_order = Some(new_order);
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }

    /// Move all selected items down one position in display order as a contiguous block.
    pub fn move_selected_down(&mut self) {
        let order = self.get_display_order();
        if order.is_empty() {
            return;
        }
        let sel_positions: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|&(_, &idx)| self.selection.get(idx).copied().unwrap_or(false))
            .map(|(pos, _)| pos)
            .collect();
        if sel_positions.is_empty() {
            return;
        }
        let sel_set: HashSet<usize> = sel_positions.iter().copied().collect();
        let last_sel = sel_positions[sel_positions.len() - 1];
        if last_sel + 1 >= order.len() {
            return;
        }
        let mut block_start = last_sel;
        while block_start > 0 && sel_set.contains(&(block_start - 1)) {
            block_start -= 1;
        }
        let mut new_order = (*order).clone();
        let item_below = new_order.remove(last_sel + 1);
        new_order.insert(block_start, item_below);
        self.custom_order = Some(new_order);
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }

    /// Move all selected items to the top of the display order,
    /// preserving their relative order.
    pub fn move_selected_to_top(&mut self) {
        let order = self.get_display_order();
        if order.is_empty() {
            return;
        }
        let mut new_order: Vec<usize> = Vec::with_capacity(order.len());
        // Selected items first (in display order, preserving relative order)
        for &idx in order.iter() {
            if self.selection.get(idx).copied().unwrap_or(false) {
                new_order.push(idx);
            }
        }
        // Unselected items after
        for &idx in order.iter() {
            if !self.selection.get(idx).copied().unwrap_or(false) {
                new_order.push(idx);
            }
        }
        if new_order == *order {
            return;
        }
        self.custom_order = Some(new_order);
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }

    /// Move all selected items to the bottom of the display order,
    /// preserving their relative order.
    pub fn move_selected_to_bottom(&mut self) {
        let order = self.get_display_order();
        if order.is_empty() {
            return;
        }
        let mut new_order: Vec<usize> = Vec::with_capacity(order.len());
        // Unselected items first
        for &idx in order.iter() {
            if !self.selection.get(idx).copied().unwrap_or(false) {
                new_order.push(idx);
            }
        }
        // Selected items after (in display order, preserving relative order)
        for &idx in order.iter() {
            if self.selection.get(idx).copied().unwrap_or(false) {
                new_order.push(idx);
            }
        }
        if new_order == *order {
            return;
        }
        self.custom_order = Some(new_order);
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }

    /// Reset to column-sorted order (clear manual reposition).
    pub fn reset_custom_order(&mut self) {
        self.custom_order = None;
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
    }

    /// Move all selected items as a contiguous block to a target display position.
    /// Used by drag-to-reorder when multiple files are selected.
    pub fn move_group_to_display_pos(&mut self, _anchor_idx: usize, target_pos: usize) {
        let order = self.get_display_order();
        if order.is_empty() {
            return;
        }
        let sel_in_order: Vec<usize> = order
            .iter()
            .filter(|&&idx| self.selection.get(idx).copied().unwrap_or(false))
            .copied()
            .collect();
        if sel_in_order.is_empty() {
            return;
        }
        let selected_before_target = order
            .iter()
            .take(target_pos)
            .filter(|&&idx| sel_in_order.contains(&idx))
            .count();
        let adjusted_target = target_pos
            .saturating_sub(selected_before_target)
            .min(order.len() - sel_in_order.len());
        let mut new_order: Vec<usize> = order
            .iter()
            .filter(|&&idx| !sel_in_order.contains(&idx))
            .copied()
            .collect();
        for (i, &idx) in sel_in_order.iter().enumerate() {
            new_order.insert(adjusted_target + i, idx);
        }
        if new_order == *order.as_ref() {
            return;
        }
        self.custom_order = Some(new_order);
        self.invalidate_sorted_cache();
        self.preview_dirty = true;
        self.rebuild_rows_cache_sync();
    }
}
