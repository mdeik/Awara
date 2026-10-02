use super::*;

pub fn rename_item_display(item: &RenameItem) -> (String, String, String) {
    match item {
        RenameItem::Prefix(v) => ("Prefix".into(), v.clone(), format!("--add-prefix {v}")),
        RenameItem::Suffix(v) => ("Suffix".into(), v.clone(), format!("--add-suffix {v}")),
        RenameItem::Insert(t, p) => (
            "Insert".into(),
            format!("{t} @ {p}"),
            format!("--add-insert {t} --add-at {p}"),
        ),
        RenameItem::WordSpace => ("Word Space".into(), "Yes".into(), "--add-word-space".into()),
        RenameItem::Regex(m, r, _full, _simple) => (
            "Regex".into(),
            format!("s/{m}/{r}"),
            format!("--regex-match {m} --regex-replace {r}"),
        ),
        RenameItem::Replace(f, t, _cs, _fr) => (
            "Replace".into(),
            format!("{f} -> {t}"),
            format!("--replace {f} --with {t}"),
        ),
        RenameItem::RemoveFirst(n) => (
            "Remove First".into(),
            n.to_string(),
            format!("--remove-first={n}"),
        ),
        RenameItem::RemoveLast(n) => (
            "Remove Last".into(),
            n.to_string(),
            format!("--remove-last={n}"),
        ),
        RenameItem::RemoveFromTo(f, t) => (
            "Remove From/To".into(),
            format!("{f} -> {t}"),
            format!("--remove-from={f} --remove-to={t}"),
        ),
        RenameItem::RemoveDigits => (
            "Remove Digits".into(),
            "Yes".into(),
            "--remove-digits".into(),
        ),
        RenameItem::RemoveChars(s) => (
            "Remove Chars".into(),
            s.clone(),
            format!("--remove-chars {s}"),
        ),
        RenameItem::RemoveWords(s, _cs) => (
            "Remove Words".into(),
            s.clone(),
            format!("--remove-words {s}"),
        ),
        RenameItem::RemoveCrop(mode, s) => {
            let tag = match mode {
                CropMode::Before => "before",
                CropMode::After => "after",
                CropMode::Special => "special",
            };
            (
                "Remove Crop".into(),
                format!("{tag}:{s}"),
                format!("--crop {tag}:{s}"),
            )
        }
        RenameItem::RemoveSymbols => (
            "Remove Symbols".into(),
            "Yes".into(),
            "--remove-symbols".into(),
        ),
        RenameItem::Trim => ("Trim".into(), "Yes".into(), "--trim".into()),
        RenameItem::DoubleSpaces => (
            "Double Spaces".into(),
            "Yes".into(),
            "--double-spaces".into(),
        ),
        RenameItem::RemoveLeadDots(mode) => (
            "Remove Lead Dots".into(),
            format!("{mode:?}"),
            format!("--remove-lead-dots {}", clap_value(*mode)),
        ),
        RenameItem::RemoveAllChars => (
            "Remove All Chars".into(),
            "Yes".into(),
            "--remove-all-chars".into(),
        ),
        RenameItem::RemoveHigh => ("Remove High".into(), "Yes".into(), "--remove high".into()),
        RenameItem::RemoveAccents => (
            "Remove Accents".into(),
            "Yes".into(),
            "--remove accents".into(),
        ),
        RenameItem::CaseName(m) => (
            "Case Name".into(),
            format!("{m:?}"),
            format!("--case-name {}", clap_value(*m)),
        ),
        RenameItem::CaseExt(_m) => (
            "Case Ext".into(),
            "(removed — use Extension)".into(),
            "(removed)".into(),
        ),
        RenameItem::Numbering(mode, start, inc, pad, sep, pos, typ, cas, brk) => {
            let mut parts = vec![format!("{mode:?} {start} inc={inc} pad={pad}")];
            if let Some(s) = sep {
                parts.push(format!("sep={s}"));
            }
            if let Some(t) = typ {
                parts.push(format!("type={t}"));
            }
            if let Some(c) = cas {
                parts.push(format!("case={c}"));
            }
            // Full flag set so the command reproduces the item exactly.
            let mut cmd = format!(
                "--numbering-mode {} --numbering-start {start} --numbering-increment={inc} \
                 --numbering-pad {pad}",
                clap_value(*mode)
            );
            if let Some(s) = sep {
                cmd.push_str(&format!(" --numbering-sep {s}"));
            }
            if let Some(p) = pos {
                cmd.push_str(&format!(" --numbering-at={p}"));
            }
            if let Some(t) = typ {
                cmd.push_str(&format!(" --numbering-type {t}"));
            }
            if let Some(c) = cas {
                cmd.push_str(&format!(" --numbering-case {c}"));
            }
            if let Some(b) = brk {
                cmd.push_str(&format!(" --numbering-break {b}"));
            }
            ("Numbering".into(), parts.join(" "), cmd)
        }
        RenameItem::Extension(mode, repl, app, rem) => {
            let mut parts = vec![format!("{mode:?}")];
            if let Some(r) = repl {
                parts.push(format!("replace={r}"));
            }
            if let Some(a) = app {
                parts.push(format!("append={a}"));
            }
            if *rem {
                parts.push("remove".into());
            }
            // Full flag set so the command reproduces the item exactly.
            let mut cmd = format!("--extension-mode {}", clap_value(*mode));
            if let Some(r) = repl {
                cmd.push_str(&format!(" --extension {r}"));
            }
            if let Some(a) = app {
                cmd.push_str(&format!(" --extension-add {a}"));
            }
            if *rem {
                cmd.push_str(" --extension-remove");
            }
            ("Extension".into(), parts.join(" "), cmd)
        }
        RenameItem::Dirname(add, sep, pos) => {
            let parts = [
                if *add { "on" } else { "off" }.to_string(),
                sep.clone().map(|s| format!("sep={s}")).unwrap_or_default(),
                pos.map(|p| format!("pos={p}")).unwrap_or_default(),
            ];
            let mut cmd = "--add-dirname".to_string();
            if let Some(s) = sep {
                cmd.push_str(&format!(" --dirname-sep {s}"));
            }
            if let Some(p) = pos {
                cmd.push_str(&format!(" --dirname-pos={p}"));
            }
            ("Dirname".into(), parts.join(" "), cmd)
        }
        RenameItem::MovePart(s, l, t, sep) => {
            let to = fmt_to(*t);
            let sep_str = sep.as_deref().unwrap_or("");
            (
                "Move Part".into(),
                format!("{s}:{l}:{to}{}", fmt_sep(sep_str)),
                format!("--move-part {s}:{l}:{to}{}", fmt_sep(sep_str)),
            )
        }
        RenameItem::CopyPart(s, l, t, sep) => {
            let to = fmt_to(*t);
            let sep_str = sep.as_deref().unwrap_or("");
            (
                "Copy Part".into(),
                format!("{s}:{l}:{to}{}", fmt_sep(sep_str)),
                format!("--copy-part {s}:{l}:{to}{}", fmt_sep(sep_str)),
            )
        }
        RenameItem::AddDate(fmt) => ("Add Date".into(), fmt.clone(), format!("--add-date {fmt}")),
        RenameItem::AddFileDate(fmt) => (
            "Add File Date".into(),
            fmt.clone(),
            format!("--add-file-date {fmt}"),
        ),
        RenameItem::ExtensionAddIfMissing => (
            "Ext Add If Missing".into(),
            "Yes".into(),
            "--extension-add-if-missing".into(),
        ),
        RenameItem::InsertMeta(s) => (
            "Insert Meta".into(),
            s.clone(),
            format!("--insert-meta {s}"),
        ),
        RenameItem::NameSegment(f, t) => (
            "Name Segment".into(),
            format!("{f}-{t}"),
            format!("--name-segment-from={f} --name-segment-to={t}"),
        ),
        RenameItem::NameFixed(v) => (
            "Name Fixed".into(),
            v.clone(),
            format!("--name-mode fixed --name-value {v}"),
        ),
        RenameItem::NameRemove => (
            "Name Remove".into(),
            "Yes".into(),
            "--name-mode remove".into(),
        ),
        RenameItem::NameReverse => (
            "Name Reverse".into(),
            "Yes".into(),
            "--name-mode reverse".into(),
        ),
        RenameItem::NamePadNumbers(amount, ch) => (
            "Name Pad Numbers".into(),
            if *ch == '0' {
                format!("{amount}")
            } else {
                format!("{amount}>{ch}")
            },
            format!(
                "--name-mode pad_numbers --name-value {}",
                if *ch == '0' {
                    amount.to_string()
                } else {
                    format!("{amount}>{ch}")
                }
            ),
        ),
        RenameItem::NameReformatDate {
            parse_fmt,
            output_fmt,
        } => {
            let val = if let Some(pf) = parse_fmt {
                format!("{pf}>{}", output_fmt)
            } else {
                output_fmt.clone()
            };
            (
                "Name Reformat Date".into(),
                val.clone(),
                format!("--name-mode reformat_date --name-value {val}"),
            )
        }
    }
}
