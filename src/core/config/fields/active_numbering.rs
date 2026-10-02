use super::*;

pub(super) fn active_fields_numbering(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = c.numbering.numbering_mode {
        items.push((
            "Numbering".into(),
            format!("{:?}", v),
            format!("--numbering-mode {:?}", v).to_lowercase(),
        ));
        items.push((
            "Num Start".into(),
            c.numbering.numbering_start.to_string(),
            format!("--numbering-start {}", c.numbering.numbering_start),
        ));
        items.push((
            "Num Inc".into(),
            c.numbering.numbering_increment.to_string(),
            format!("--numbering-increment {}", c.numbering.numbering_increment),
        ));
        items.push((
            "Num Pad".into(),
            c.numbering.numbering_pad.to_string(),
            format!("--numbering-pad {}", c.numbering.numbering_pad),
        ));
        if let Some(sep) = &c.numbering.numbering_sep {
            items.push((
                "Num Sep".into(),
                sep.clone(),
                format!("--numbering-sep {}", sep),
            ));
        }
        if let Some(v) = c.numbering.numbering_at
            && !opt_num_default(Some(v))
        {
            items.push((
                "Num At".into(),
                v.to_string(),
                format!("--numbering-at {}", v),
            ));
        }
        if let Some(v) = c.numbering.numbering_break
            && !opt_num_default(Some(v))
        {
            items.push((
                "Num Break".into(),
                v.to_string(),
                format!("--numbering-break {}", v),
            ));
        }
        if c.numbering.numbering_restart_folder {
            items.push((
                "Num Restart Folder".into(),
                "Yes".into(),
                "--numbering-restart-folder".into(),
            ));
        }
        if let Some(v) = &c.numbering.numbering_type {
            items.push((
                "Num Type".into(),
                v.clone(),
                format!("--numbering-type {}", v),
            ));
        }
        if let Some(v) = &c.numbering.numbering_case {
            items.push((
                "Num Case".into(),
                v.clone(),
                format!("--numbering-case {}", v),
            ));
        }
    }
    if let Some(v) = c.numbering.numbering_source {
        items.push((
            "Num Source".into(),
            format!("{:?}", v),
            format!("--numbering-source {}", clap_value(v)),
        ));
    }
}

pub(super) fn active_fields_extension(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = c.extension.extension_mode {
        items.push((
            "Ext Mode".into(),
            format!("{:?}", v),
            format!("--extension-mode {:?}", v).to_lowercase(),
        ));
    }
    if let Some(v) = &c.extension.extension_replace {
        items.push((
            "Ext Replace".into(),
            v.clone(),
            format!("--extension {}", v),
        ));
    }
    if let Some(v) = &c.extension.extension_append {
        items.push((
            "Ext Add".into(),
            v.clone(),
            format!("--extension-add {}", v),
        ));
    }
    if c.extension.extension_remove {
        items.push((
            "Ext Remove".into(),
            "Yes".into(),
            "--extension-remove".into(),
        ));
    }
    if c.extension.extension_add_if_missing {
        items.push((
            "Ext Add If Missing".into(),
            "Yes".into(),
            "--extension-add-if-missing".into(),
        ));
    }
}

pub(super) fn active_fields_dirname(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if c.append_folder.add_dirname {
        items.push(("Add Dirname".into(), "Yes".into(), "--add-dirname".into()));
    }
    if let Some(v) = &c.append_folder.add_dirname_sep
        && !v.is_empty()
    {
        items.push((
            "Dirname Sep".into(),
            v.clone(),
            format!("--dirname-sep {}", v),
        ));
    }
    if let Some(v) = c.append_folder.add_dirname_pos {
        items.push((
            "Dirname Pos".into(),
            v.to_string(),
            format!("--dirname-pos {}", v),
        ));
    }
    if c.append_folder.dirname_level > 1 {
        items.push((
            "Dirname Level".into(),
            c.append_folder.dirname_level.to_string(),
            format!("--dirname-level {}", c.append_folder.dirname_level),
        ));
    }
}

pub(super) fn active_fields_move_copy(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.move_copy.move_part {
        let sep_str = v.separator.as_deref().unwrap_or("");
        items.push((
            "Move Part".into(),
            format!(
                "{}:{}:{}{}",
                v.start,
                v.length,
                fmt_to(v.destination),
                fmt_sep(sep_str)
            ),
            format!(
                "--move-part {}:{}:{}{}",
                v.start,
                v.length,
                fmt_to(v.destination),
                fmt_sep(sep_str)
            ),
        ));
    }
    if let Some(v) = &c.move_copy.copy_part {
        let sep_str = v.separator.as_deref().unwrap_or("");
        items.push((
            "Copy Part".into(),
            format!(
                "{}:{}:{}{}",
                v.start,
                v.length,
                fmt_to(v.destination),
                fmt_sep(sep_str)
            ),
            format!(
                "--copy-part {}:{}:{}{}",
                v.start,
                v.length,
                fmt_to(v.destination),
                fmt_sep(sep_str)
            ),
        ));
    }
}

pub(super) fn active_fields_date(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.auto_date.add_date {
        let pos_label = if c.auto_date.date_position == DatePosition::Suffix {
            " (suffix)"
        } else {
            ""
        };
        items.push((
            format!("Add Date{}", pos_label),
            v.clone(),
            format!("--add-date {}", v),
        ));
    }
    if let Some(v) = &c.auto_date.add_file_date {
        let pos_label = if c.auto_date.date_position == DatePosition::Suffix {
            " (suffix)"
        } else {
            ""
        };
        items.push((
            format!("Add File Date{}", pos_label),
            v.clone(),
            format!("--add-file-date {}", v),
        ));
    }
}

pub(super) fn active_fields_exclude(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.filters.exclude_regex
        && !v.is_empty()
    {
        items.push((
            "Exclude Regex".into(),
            v.clone(),
            format!("--exclude-regex {}", v),
        ));
    }
}

pub(super) fn active_fields_metadata(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.auto_date.insert_meta {
        items.push((
            "Insert Meta".into(),
            v.clone(),
            format!("--insert-meta {}", v),
        ));
    }
}
