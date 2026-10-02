use super::*;

pub(super) fn active_fields_replace(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.replace.replace
        && !v.is_empty()
    {
        let w = c.replace.with.as_deref().unwrap_or("");
        items.push((
            "Replace".into(),
            format!("{} -> {}", v, w),
            format!("--replace {} --with {}", v, w),
        ));
    }
    if c.replace.replace_case_sensitive {
        items.push((
            "Replace Case Sensitive".into(),
            "Yes".into(),
            "--replace-case-sensitive".into(),
        ));
    }
    if c.replace.replace_first {
        items.push((
            "Replace First".into(),
            "Yes".into(),
            "--replace-first".into(),
        ));
    }
}

pub(super) fn active_fields_regex(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.regex.regex_match
        && !v.is_empty()
    {
        let r = c.regex.regex_replace.as_deref().unwrap_or("");
        items.push((
            "Regex".into(),
            format!("s/{}/{}", v, r),
            format!("--regex-match {} --regex-replace {}", v, r),
        ));
    }
    if c.regex.regex_full_name {
        items.push((
            "Regex Full Name".into(),
            "Yes".into(),
            "--regex-full-name".into(),
        ));
    }
}

pub(super) fn active_fields_case(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = c.case.case_name {
        items.push((
            "Case Name".into(),
            format!("{:?}", v),
            format!("--case-name {}", clap_value(v)),
        ));
    }
    // case_ext removed — use Extension section instead
}

pub(super) fn active_fields_remove(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = c.remove.remove_first
        && !opt_num_default(Some(v))
    {
        items.push((
            "Remove First".into(),
            v.to_string(),
            format!("--remove-first={v}"),
        ));
    }
    if let Some(v) = c.remove.remove_last
        && !opt_num_default(Some(v))
    {
        items.push((
            "Remove Last".into(),
            v.to_string(),
            format!("--remove-last={v}"),
        ));
    }
    if let Some(v) = c.remove.remove_from
        && !opt_num_default(Some(v))
    {
        items.push((
            "Remove From".into(),
            v.to_string(),
            format!("--remove-from={v}"),
        ));
    }
    if let Some(v) = c.remove.remove_to
        && !opt_num_default(Some(v))
    {
        items.push((
            "Remove To".into(),
            v.to_string(),
            format!("--remove-to={v}"),
        ));
    }
    if c.remove.remove_digits {
        items.push((
            "Remove Digits".into(),
            "Yes".into(),
            "--remove-digits".into(),
        ));
    }
    if let Some(v) = &c.remove.remove_chars
        && !v.is_empty()
    {
        items.push((
            "Remove Chars".into(),
            v.clone(),
            format!("--remove-chars {}", v),
        ));
    }
    if let Some(v) = &c.remove.remove_words
        && !v.is_empty()
    {
        items.push((
            "Remove Words".into(),
            v.clone(),
            format!("--remove-words {}", v),
        ));
    }
    if c.remove.remove_symbols {
        items.push((
            "Remove Symbols".into(),
            "Yes".into(),
            "--remove-symbols".into(),
        ));
    }
    if c.remove.trim {
        items.push(("Trim".into(), "Yes".into(), "--trim".into()));
    }
    if c.remove.double_spaces {
        items.push((
            "Double Spaces".into(),
            "Yes".into(),
            "--double-spaces".into(),
        ));
    }
    if c.remove.remove_high {
        items.push(("Remove High".into(), "Yes".into(), "--remove high".into()));
    }
    if c.remove.remove_accents {
        items.push((
            "Remove Accents".into(),
            "Yes".into(),
            "--remove accents".into(),
        ));
    }
    if let Some(v) = c.remove.remove_lead_dots {
        items.push((
            "Remove Lead Dots".into(),
            format!("{:?}", v),
            format!("--remove-lead-dots {:?}", v).to_lowercase(),
        ));
    }
    if c.remove.remove_all_chars {
        items.push((
            "Remove All Chars".into(),
            "Yes".into(),
            "--remove-all-chars".into(),
        ));
    }
    if let (Some(mode), Some(s)) = (c.remove.crop_mode, &c.remove.remove_crop)
        && !s.is_empty()
    {
        let tag = match mode {
            CropMode::Before => "before",
            CropMode::After => "after",
            CropMode::Special => "special",
        };
        items.push((
            "Remove Crop".into(),
            format!("{}: '{}'", tag, s),
            format!("--crop {}:{}", tag, s),
        ));
    }
}

pub(super) fn active_fields_add(c: &RenameConfig, items: &mut Vec<(String, String, String)>) {
    if let Some(v) = &c.add.add_prefix
        && !v.is_empty()
    {
        items.push(("Prefix".into(), v.clone(), format!("--add-prefix {}", v)));
    }
    if let Some(v) = &c.add.add_suffix
        && !v.is_empty()
    {
        items.push(("Suffix".into(), v.clone(), format!("--add-suffix {}", v)));
    }
    if let Some(v) = &c.add.add_insert
        && !v.is_empty()
    {
        items.push(("Insert".into(), v.clone(), format!("--add-insert {}", v)));
    }
    if let Some(v) = c.add.add_at
        && !opt_num_default(Some(v))
    {
        items.push(("Insert At".into(), v.to_string(), format!("--add-at {}", v)));
    }
    if c.add.add_word_space {
        items.push(("Word Space".into(), "Yes".into(), "--add-word-space".into()));
    }
}
