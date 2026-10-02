use super::*;

pub(super) fn active_fields_exec_flags(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if c.preserve_timestamps {
        items.push((
            "Preserve Timestamps".into(),
            "Yes".into(),
            "--preserve-timestamps".into(),
        ));
    }
    if let Some(v) = &c.special.set_attributes {
        items.push((
            "Set Attributes".into(),
            v.clone(),
            format!("--set-attributes {}", v),
        ));
    }
    if let Some(v) = &c.special.set_created {
        items.push((
            "Set Created".into(),
            v.to_cli_str(),
            format!("--set-created {}", v.to_cli_str()),
        ));
    }
    if let Some(v) = &c.special.set_modified {
        items.push((
            "Set Modified".into(),
            v.to_cli_str(),
            format!("--set-modified {}", v.to_cli_str()),
        ));
    }
    if let Some(v) = &c.special.set_accessed {
        items.push((
            "Set Accessed".into(),
            v.to_cli_str(),
            format!("--set-accessed {}", v.to_cli_str()),
        ));
    }
    if c.special.timestamp_incr_secs > 0 {
        items.push((
            "TS Incr".into(),
            format!("{}s/file", c.special.timestamp_incr_secs),
            String::new(),
        ));
    }
    if c.copy_to.keep_structure {
        items.push((
            "Keep Structure".into(),
            "Yes".into(),
            "--keep-structure".into(),
        ));
    }
}

pub(super) fn active_fields_filters(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = c.filters.min_name_len
        && !opt_num_default(Some(v))
    {
        items.push((
            "Min Name Len".into(),
            v.to_string(),
            format!("--min-name-len {}", v),
        ));
    }
    if let Some(v) = c.filters.max_name_len
        && !opt_num_default(Some(v))
    {
        items.push((
            "Max Name Len".into(),
            v.to_string(),
            format!("--max-name-len {}", v),
        ));
    }
    if let Some(v) = c.filters.min_path_len
        && !opt_num_default(Some(v))
    {
        items.push((
            "Min Path Len".into(),
            v.to_string(),
            format!("--min-path-len {}", v),
        ));
    }
    if let Some(v) = c.filters.max_path_len
        && !opt_num_default(Some(v))
    {
        items.push((
            "Max Path Len".into(),
            v.to_string(),
            format!("--max-path-len {}", v),
        ));
    }
    if let Some(v) = &c.filters.filter_attr {
        items.push((
            "Filter Attr".into(),
            v.clone(),
            format!("--filter-attr {}", v),
        ));
    }
    if c.filters.filter_files && !c.filters.filter_folders {
        items.push(("Filter Files".into(), "Yes".into(), "--filter-files".into()));
    }
    if c.filters.filter_folders && !c.filters.filter_files {
        items.push((
            "Filter Folders".into(),
            "Yes".into(),
            "--filter-folders".into(),
        ));
    }
    if c.filters.filter_hidden {
        items.push((
            "Filter Hidden".into(),
            "Yes".into(),
            "--filter-hidden".into(),
        ));
    }
    if c.filters.filter_subfolders {
        items.push((
            "Filter Subfolders".into(),
            "Yes".into(),
            "--filter-subfolders".into(),
        ));
    }
    if let Some(v) = c.filters.filter_level
        && !opt_num_default(Some(v))
    {
        items.push((
            "Filter Level".into(),
            v.to_string(),
            format!("--filter-level {}", v),
        ));
    }
    if c.filters.filter_use_regex {
        items.push((
            "Filter Use Regex".into(),
            "Yes".into(),
            "--filter-use-regex".into(),
        ));
    }
    if c.filters.filter_match_case {
        items.push((
            "Filter Match Case".into(),
            "Yes".into(),
            "--filter-match-case".into(),
        ));
    }
}

pub(super) fn active_fields_name_mode(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.name.name_mode {
        items.push(("Name Mode".into(), v.clone(), format!("--name-mode {}", v)));
        if let Some(val) = &c.name.name_value
            && !val.is_empty()
        {
            items.push((
                "Name Value".into(),
                val.clone(),
                format!("--name-value {}", val),
            ));
        }
    }
}

pub(super) fn active_fields_case_exception(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if let Some(v) = &c.case.case_exception
        && !v.is_empty()
    {
        items.push((
            "Case Exception".into(),
            v.clone(),
            format!("--case-exception {}", v),
        ));
    }
}

pub(super) fn active_fields_regex_mode(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if c.regex.regex_simple {
        items.push(("Regex Simple".into(), "Yes".into(), "--regex-simple".into()));
    }
}

pub(super) fn active_fields_name_segment(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if c.name_segment.copy_name_segment_from != 0 || c.name_segment.copy_name_segment_to != 0 {
        items.push((
            "Name Segment".into(),
            format!(
                "{}:{}",
                c.name_segment.copy_name_segment_from, c.name_segment.copy_name_segment_to
            ),
            format!(
                "--name-segment-from={} --name-segment-to={}",
                c.name_segment.copy_name_segment_from, c.name_segment.copy_name_segment_to
            ),
        ));
    }
}

pub(super) fn active_fields_date_position(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if c.auto_date.date_position == DatePosition::Suffix {
        items.push((
            "Date Position".into(),
            "Suffix".into(),
            "--date-position suffix".into(),
        ));
    }
}

pub(super) fn active_fields_keep_structure(
    c: &RenameConfig,
    items: &mut Vec<(String, String, String)>,
) {
    if c.copy_to.keep_structure {
        items.push((
            "Keep Structure".into(),
            "Yes".into(),
            "--keep-structure".into(),
        ));
    }
}

pub(super) fn active_fields_output(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.copy_to.output_dir
        && !v.is_empty()
    {
        items.push((
            "Output Dir".into(),
            v.clone(),
            format!("--output-dir {}", v),
        ));
    }
    if c.copy_to.copy_mode {
        items.push(("Copy Mode".into(), "Yes".into(), "--copy-mode".into()));
    }
}
