use super::*;

pub fn config_to_items(config: &RenameConfig) -> Vec<RenameItem> {
    emit_section_items(config, SectionId::default_order().iter().copied())
}

/// Emit `RenameItem`s for the given sequence of sections.
/// Shared by `config_to_items` (which passes the default order) and callers
/// that expand a single section at a time (the GUI's `build_command_order`).
pub fn emit_section_items(
    config: &RenameConfig,
    sections: impl Iterator<Item = SectionId>,
) -> Vec<RenameItem> {
    let c = config;
    let mut items = Vec::new();
    for section in sections {
        match section {
            SectionId::NameSegment => {
                let from = c.name_segment.copy_name_segment_from;
                let to = c.name_segment.copy_name_segment_to;
                // Any non-default pair produces an item; `to = 0` means
                // "through the end of the name" (same as Remove From/To).
                if from != 0 || to != 0 {
                    items.push(RenameItem::NameSegment(from, to));
                }
            }
            SectionId::Regex => {
                if let Some(item) = make_regex_item(c) {
                    items.push(item);
                }
            }
            SectionId::Extension => {
                if let Some(item) = make_extension_item(c) {
                    items.push(item);
                }
                if c.extension.extension_add_if_missing {
                    items.push(RenameItem::ExtensionAddIfMissing);
                }
            }
            SectionId::Replace => {
                if let Some(item) = make_replace_item(c) {
                    items.push(item);
                }
            }
            SectionId::Name => {
                items.extend(make_name_items(c));
            }
            SectionId::Remove => {
                items.extend(make_remove_items(c));
            }
            SectionId::MoveCopy => {
                items.extend(make_move_copy_items(c));
            }
            SectionId::Add => {
                items.extend(make_add_items(c));
                if c.add.add_word_space {
                    items.push(RenameItem::WordSpace);
                }
            }
            SectionId::Numbering => {
                if let Some(item) = make_numbering_item(c) {
                    items.push(item);
                }
            }
            SectionId::AppendFolder => {
                if let Some(item) = make_dirname_item(c) {
                    items.push(item);
                }
            }
            SectionId::Case => {
                items.extend(make_case_items(c));
            }
            SectionId::AutoDate => {
                items.extend(make_auto_date_items(c));
            }
            // Filters, CopyTo, Special don't produce RenameItems — skip.
            SectionId::Filters | SectionId::CopyTo | SectionId::Special => {}
        }
    }
    items
}

/// Flatten a RenameItem into RenameConfig named fields (stacked → named).
pub fn items_to_config(items: Vec<RenameItem>, cfg: &mut RenameConfig) {
    for item in items {
        match item {
            RenameItem::Prefix(v) => cfg.add.add_prefix = Some(v),
            RenameItem::Suffix(v) => cfg.add.add_suffix = Some(v),
            RenameItem::Regex(m, r, f, simple) => {
                cfg.regex.regex_match = Some(m);
                cfg.regex.regex_replace = Some(r);
                cfg.regex.regex_full_name = f;
                cfg.regex.regex_simple = simple;
            }
            RenameItem::Replace(f, w, c, rf) => {
                cfg.replace.replace = Some(f);
                cfg.replace.with = Some(w);
                cfg.replace.replace_case_sensitive = c;
                cfg.replace.replace_first = rf;
            }
            RenameItem::RemoveFirst(n) => cfg.remove.remove_first = Some(n),
            RenameItem::RemoveLast(n) => cfg.remove.remove_last = Some(n),
            RenameItem::RemoveFromTo(f, t) => {
                cfg.remove.remove_from = Some(f);
                cfg.remove.remove_to = Some(t);
            }
            RenameItem::RemoveDigits => cfg.remove.remove_digits = true,
            RenameItem::RemoveChars(s) => cfg.remove.remove_chars = Some(s),
            RenameItem::RemoveWords(s, cs) => {
                cfg.remove.remove_words = Some(s);
                cfg.replace.replace_case_sensitive = cs;
            }
            RenameItem::RemoveSymbols => cfg.remove.remove_symbols = true,
            RenameItem::Trim => cfg.remove.trim = true,
            RenameItem::DoubleSpaces => cfg.remove.double_spaces = true,
            RenameItem::RemoveHigh => cfg.remove.remove_high = true,
            RenameItem::RemoveAccents => cfg.remove.remove_accents = true,
            RenameItem::Insert(t, p) => {
                cfg.add.add_insert = Some(t);
                cfg.add.add_at = Some(p);
            }
            RenameItem::WordSpace => cfg.add.add_word_space = true,
            RenameItem::CaseName(m) => cfg.case.case_name = Some(m),
            RenameItem::CaseExt(_m) => {
                // case_ext removed — use Extension section instead
            }
            RenameItem::Numbering(mode, start, inc, pad, sep, pos, typ, cas, brk) => {
                cfg.numbering.numbering_mode = Some(mode);
                cfg.numbering.numbering_start = start;
                cfg.numbering.numbering_break = brk;
                cfg.numbering.numbering_increment = inc;
                cfg.numbering.numbering_pad = pad;
                cfg.numbering.numbering_sep = sep;
                cfg.numbering.numbering_at = pos;
                cfg.numbering.numbering_type = typ;
                cfg.numbering.numbering_case = cas;
            }
            RenameItem::Extension(mode, repl, app, rem) => {
                cfg.extension.extension_mode = Some(mode);
                cfg.extension.extension_replace = repl;
                cfg.extension.extension_append = app;
                cfg.extension.extension_remove = rem;
            }
            RenameItem::Dirname(add, sep, pos) => {
                cfg.append_folder.add_dirname = add;
                cfg.append_folder.add_dirname_sep = sep;
                cfg.append_folder.add_dirname_pos = pos;
            }
            RenameItem::MovePart(s, l, t, sep) => {
                cfg.move_copy.move_part = Some(MoveCopyValue {
                    start: s,
                    length: l,
                    destination: t,
                    separator: sep,
                })
            }
            RenameItem::CopyPart(s, l, t, sep) => {
                cfg.move_copy.copy_part = Some(MoveCopyValue {
                    start: s,
                    length: l,
                    destination: t,
                    separator: sep,
                })
            }
            RenameItem::AddDate(fmt) => cfg.auto_date.add_date = Some(fmt),
            RenameItem::AddFileDate(fmt) => cfg.auto_date.add_file_date = Some(fmt),
            RenameItem::ExtensionAddIfMissing => cfg.extension.extension_add_if_missing = true,
            RenameItem::InsertMeta(s) => cfg.auto_date.insert_meta = Some(s),
            RenameItem::RemoveCrop(mode, s) => {
                cfg.remove.crop_mode = Some(mode);
                cfg.remove.remove_crop = Some(s);
            }
            RenameItem::RemoveLeadDots(mode) => cfg.remove.remove_lead_dots = Some(mode),
            RenameItem::RemoveAllChars => cfg.remove.remove_all_chars = true,
            RenameItem::NameSegment(f, t) => {
                cfg.name_segment.copy_name_segment_from = f;
                cfg.name_segment.copy_name_segment_to = t;
            }
            RenameItem::NameFixed(v) => {
                cfg.name.name_mode = Some("fixed".into());
                cfg.name.name_value = Some(v);
            }
            RenameItem::NameRemove => {
                cfg.name.name_mode = Some("remove".into());
                cfg.name.name_value = None;
            }
            RenameItem::NameReverse => {
                cfg.name.name_mode = Some("reverse".into());
                cfg.name.name_value = None;
            }
            RenameItem::NamePadNumbers(amount, char) => {
                cfg.name.name_mode = Some("pad_numbers".into());
                cfg.name.name_value = Some(if char == '0' {
                    amount.to_string()
                } else {
                    format!("{}>{}", amount, char)
                });
            }
            RenameItem::NameReformatDate {
                parse_fmt,
                output_fmt,
            } => {
                cfg.name.name_mode = Some("reformat_date".into());
                cfg.name.name_value = Some(if let Some(pf) = parse_fmt {
                    format!("{}>{}", pf, output_fmt)
                } else {
                    output_fmt
                });
            }
        }
    }
}
