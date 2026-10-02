# CLI Reference

The CLI is designed for scripting, automation, and quick one-off renames from the
terminal. Every rename operation is a command-line flag — there is no interactive setup
prompt (the only prompt is the collision confirmation described below). Running with no
arguments launches the **GUI** (if supported) or the **TUI** as fallback; passing flags
but no `FILES` prints help and exits.

```
awara [OPTIONS] [FILES]...
```

---

## Usage

The order of flags doesn't matter — operations run in a fixed internal order matching
the GUI/TUI default section order:

Name Segment → RegEx → Extension → Replace → Name → Remove → Move / Copy Parts →
Add → Numbering → Append Folder Name → Case → Auto Date

The GUI and TUI can reorder sections per operation (GUI: *Special* → *Order*). Run with
`--dry-run` / `-n` to preview before committing:

```bash
# Preview changes before renaming
awara *.txt --replace "old" --with "new" --dry-run

# Apply the rename
awara *.txt --replace "old" --with "new"
```

---

## Positional Arguments

| Argument | Description |
|---|---|
| `FILES`... | Files to process (supports glob patterns) |

---

## General Options

| Flag | Short | Description |
|---|---|---|
| `--dry-run` | `-n` | Dry run — don't rename, just print what would happen |
| `--verbose` | `-v` | Verbose output |
| `--recursive` | `-r` | Search directories recursively |
| `--max-depth <n>` | | Max recursion depth |
| `--stop-on-error` | | Stop on first error and roll back renames already applied in the batch |
| `--output-dir <dir>` | | Output directory (copy/move renamed files here) |
| `--copy-mode` | | Copy instead of move (when using `--output-dir`) — this is already the default |
| `--move` | | Move instead of copy (when using `--output-dir`) |
| `--overwrite` | | Overwrite existing target files (applies to all collisions, no prompt). Existing directories are never overwritten or merged and are skipped |
| `--skip` | | Skip existing target files (applies to all collisions, no prompt) |
| `--undo-file <path>` | | Write a JSON undo record of the successful **in-place renames** to the given path. Copy / Move to Location (`--output-dir`) transfers, and renames whose new name contains a path separator (which create a subfolder), are not recorded |
| `--apply-undo <path>` | | Apply a JSON undo record produced by `--undo-file`, reverting the recorded renames |
| `--preserve-timestamps` | | Preserve file timestamps after rename |
| `--set-attributes <a>` | | Set/clear file attributes (comma-separated `name`/`+name` to set, `-name` to clear): `readonly`, `hidden`, `system`, `archive` |
| `--keep-structure` | | Keep directory structure when copying to output-dir |

> **Copy/move only affects the selected entries.** A regular file is copied (copy
> mode) or moved (move mode). A **directory relocated to the output location is
> always recreated empty** — under its (renamed) name, with its source kept — in
> *both* modes, because moving it would drag along unselected contents. Only an
> in-place directory rename moves the directory itself. Entries that were not
> selected/scanned are never touched.

> **A computed name may only create subfolders — never navigate.** A name like
> `sub/file.txt` creates `sub`; but a rooted/absolute name (`/file.txt`) or one
> containing a `.` or `..` segment (`./x`, `../x`, `sub/../x`) invalidates the
> whole batch: **nothing is applied** and the offending names are reported as
> errors. This keeps a rename from escaping its folder.

**Attribute support is resolved per volume, not globally per OS.** Each attribute is
applied through the mechanism the target volume actually offers (on Windows the Win32
attribute API is always used):

| Volume | `readonly` | `hidden` | `system` / `archive` |
|---|---|---|---|
| Windows (any) | permission bit | DOS hidden bit (no rename) | DOS bits |
| Linux ext4/xfs/btrfs/… | permission bit | leading `.` (renames the file) | — |
| Linux vfat/exFAT | permission bit | FAT hidden attribute (no rename) | FAT bits |
| Linux SMB/CIFS · ntfs3/ntfs-3g | permission bit | `user.DOSATTRIB` xattr | `user.DOSATTRIB` xattr |
| macOS APFS/HFS+ | permission bit | `UF_HIDDEN` flag (no rename) | — |

Unsupported attributes are skipped and never faked; a note is reported once per
distinct set of missing attributes. `hidden` falls back to a leading dot only where the
volume has no usable hidden flag (native Unix) — see below.

**`--set-attributes` syntax.** Comma-separated: a bare `name` or `+name` sets the
attribute, `-name` clears it. `readonly` and `read-only` are equivalent. Attributes
are applied to the file's **final** name, i.e. after any rename or copy.

**`hidden` on a filesystem with no hidden flag** (native Unix — ext4/xfs/btrfs/
tmpfs/overlay/NFS). There the attribute *is* the name, so `hidden` prepends `.`
and `-hidden` strips it:

- applying `hidden` to a name already starting with `.` is a no-op (no second dot);
- `-hidden` strips **all** leading dots (`.log` → `log`, `..log` → `log`);
- a name made only of dots (`...`) cannot be unhidden and is left as-is;
- because it acts on the final name, a rename that adds a leading dot together
  with `-hidden` cancels out, and a rename that already yields a dot-prefixed name
  makes `hidden` a no-op.

On Windows, vfat/exFAT, SMB/CIFS, and macOS the hidden flag is metadata, so none of
that name rewriting applies.

> On **all** Unix platforms a leading `.` is also treated as hidden when scanning
> and filtering (so `--filter-hidden` includes dotfiles). Only the native-Unix dot
> mechanism *removes* it: on vfat/exFAT, SMB/CIFS, and macOS, `-hidden` clears the
> metadata flag and leaves any dot in place — a dotfile there stays hidden.

When a rename would overwrite an existing file and no `--overwrite`/`--skip` flag
was passed, the CLI prompts `Overwrite all? [y/N]` (answer `n` skips). In
non-interactive contexts (piped stdin) it skips collisions and prints a warning.

A target that is an existing **directory** is never overwritten or merged, even
with `--overwrite`: folders have no atomic replace, so such targets are always
skipped (and reported) rather than deleted. The rule is broader still — folder
handling applies whenever a folder is involved on **either** side, so a **folder
source** is never allowed to replace an existing file either. Only a plain
**file → file** collision can be overwritten.

A file whose name does not change is never renamed (no self-renames). It is
still processed when `--output-dir` is set (copied/moved) or when a requested
effect can actually be applied on the target filesystem; otherwise it is skipped.
For example, setting `system`/`archive` on a native ext4/APFS volume, or birth
time (`--set-created`) on Linux, does nothing, so a name-unchanged file is not
reported as processed. Capability is resolved per volume at execution time.

---

## Replace Operations

| Flag | Description |
|---|---|
| `--replace <find>` | String to find |
| `--with <replacement>` | Replacement string |
| `--replace-case-sensitive` | Case sensitive matching for replace and remove-words (regex is always case-sensitive; use `(?i)`) |
| `--replace-first` | Replace only the first occurrence |

---

## Regex Operations

| Flag | Description |
|---|---|
| `--regex-match <pattern>` | Regex match pattern |
| `--regex-replace <replacement>` | Regex replacement pattern |
| `--regex-full-name` | Apply regex to full filename (including extension) |
| `--regex-simple <pattern>` | Simplified regex match pattern (no need to escape special chars) |
| `--exclude-regex <pattern>` | Exclude files matching regex pattern |

---

## Remove Operations

| Flag | Description |
|---|---|
| `--remove-first <n>` | Remove first N characters |
| `--remove-last <n>` | Remove last N characters |
| `--remove-from <n>` | Remove from position N |
| `--remove-to <n>` | Remove to position N |
| `--remove-chars <s>` | Remove specific characters |
| `--remove-words <s>` | Remove specific words |
| `--remove-digits` | Remove all digits |
| `--remove-symbols` | Remove symbols |
| `--remove-all-chars` | Remove all characters in the name part, leaving extension intact |
| `--trim` | Trim leading/trailing whitespace |
| `--double-spaces` | | Collapse runs of whitespace to a single space |
| `--remove <mode>` | Extended remove modes (can be repeated): `digits`, `high`, `trim`, `double-spaces`, `accents`, `symbols` |
| `--remove-lead-dots <mode>` | Remove leading dots: `single`, `double`, `both` |
| `--crop <spec>` | Crop text: `before:TEXT` (keep text after the match), `after:TEXT` (keep text before the match), or `special:TEXT` (keep only the match) |

---

## Add / Insert Operations

| Flag | Description |
|---|---|
| `--add-prefix <s>` | Add prefix |
| `--add-suffix <s>` | Add suffix |
| `--add-insert <s>` | Insert text |
| `--add-at <n>` | Insert at position N (negative counts from the end, e.g. -1 = before last char) |
| `--add-word-space` | Add space between words (CamelCase → Camel Case) |
| `--add-dirname` | Prepend parent directory name |
| `--dirname-sep <s>` | Separator between dirname and filename |
| `--dirname-pos <n>` | Dirname insertion position (0 = start) |
| `--dirname-level <n>` | | Number of parent folders to add (1 = immediate parent; higher levels add ancestors, ordered grandparent → parent; default: 1) |

---

## Name Mode

| Flag | Description |
|---|---|
| `--name-mode <mode>` | Set name mode: `keep`, `fixed`, `remove`, `reverse`, `pad_numbers`, `reformat_date` |
| `--name-value <value>` | Value for `--name-mode`: `fixed` = whole name; `pad_numbers` = width[`>char`]; `reformat_date` = [`parse>`]output format |
| `--name-segment-from <n>` | Copy name segment from position (use with `--name-segment-to`) |
| `--name-segment-to <n>` | Copy name segment to position (use with `--name-segment-from`) |

---

## Numbering

| Flag | Description |
|---|---|
| `--numbering-mode <mode>` | Numbering mode: `prefix`, `suffix`, `both`, `insert` |
| `--numbering-start <n>` | Starting number (default: 1) |
| `--numbering-increment <n>` | Increment step (default: 1) |
| `--numbering-pad <n>` | Zero-pad to width N (default: 0 = no padding) |
| `--numbering-sep <s>` | Separator between number and filename |
| `--numbering-source <s>` | Sort source for numbering order: `name`, `date`, `size` |
| `--numbering-at <n>` | Insert numbering at character position |
| `--numbering-break <n>` | Reset numbering every N files |
| `--numbering-restart-folder` | Restart numbering for each folder when renaming recursively |
| `--numbering-type <type>` | Numbering type: `decimal`, `hex`, `octal`, `az_upper`, `az_lower`, `alpha`, `roman`, or a base number (2–36) |
| `--numbering-case <case>` | Numbering letter case: `lower`, `upper` |

Numbering runs at its position in the operation order (after *Add*, before *Append
Folder Name* by default); reordering the sections changes that.

---

## Case Operations

| Flag | Description |
|---|---|
| `--case-name <mode>` | Case conversion for filename: `lower`, `upper`, `title`, `title-enhanced`, `sentence`, `invert` |
| `--case-exception <word>` | Case exception word (preserved as-is during case conversion) |

---

## Extension Operations

| Flag | Description |
|---|---|
| `--extension-mode <mode>` | Extension mode: `lower`, `upper`, `title` |
| `--extension <ext>` | Replace the extension (e.g. `--extension txt`). The value replaces it verbatim — no leading dot — and is lowercased unless `--extension-mode` is set |
| `--extension-add <s>` | Append to extension |
| `--extension-remove` | Remove extension entirely |
| `--extension-add-if-missing` | Add the extension only when the file has none *(not yet honored by the engine — see `MISSING_FEATURES.md`)* |

---

## Part Operations

| Flag | Description |
|---|---|
| `--move-part <s:l:to[:sep]>` | Move substring: `start:length:to_position[:separator]`. Positions are 1-based; negative counts from the end. Use `end` for the very end (after the last character); the optional separator is inserted after the moved text (before it when it lands at the very end) |
| `--copy-part <s:l:to[:sep]>` | Copy substring: `start:length:to_position[:separator]`. Positions are 1-based; negative counts from the end. Use `end` for the very end (after the last character); the optional separator is inserted after the copied text (before it when it lands at the very end) |

---

## Date / Metadata

| Flag | Description |
|---|---|
| `--add-date <fmt>` | Insert current date with format (e.g. `YYYY-MM-DD`); defaults to a prefix |
| `--add-file-date <fmt>` | Insert file modification date with format; defaults to a prefix |
| `--date-position <pos>` | Date position: `none`, `prefix`, or `suffix` (default: `none`, or `prefix` when a date field is set) |
| `--insert-meta <tag>` | Insert metadata tag (defaults to a prefix): `exif-date`, `taken_original`, `modified`, `taken_modified`, `taken_digitized`, `taken_recent` |
| `--auto-date-century` | Include the century (YYYY) in auto-date output |
| `--auto-date-offset <hours>` | Add extra hours to auto-date, on top of local time |
| `--auto-date-sep-filename <s>` | Separator between filename and auto-date |
| `--set-created <spec>` | Set creation time after rename. Value: `current`, `taken`, `delta:<±secs>`, `copy-from:created\|modified\|accessed`, or a local datetime `YYYY-MM-DD[ HH:MM[:SS]][ AM\|PM]` (the `fixed:` prefix is optional). Only settable where the platform/volume exposes a birth time (**Windows**, **macOS**); it is a no-op on Linux |
| `--set-modified <spec>` | Set modification time after rename (same syntax as `--set-created`) |
| `--set-accessed <spec>` | Set access time after rename (same syntax as `--set-created`) |

> ⏰ **Time zone:** dates and timestamps use your **local time zone** by default; `--auto-date-offset` adds extra hours on top of it. Fixed values (`fixed:2024-01-15 14:30:00`) are treated as local time and converted to UTC when written to the filesystem. A 12-hour clock with an optional `AM`/`PM` suffix is accepted and normalized to 24-hour form (the hour must be 1–12). Invalid timestamps are **rejected with an error**, never silently ignored or saved.

---

## Sorting

| Flag | Description |
|---|---|
| `--sort <mode>` | Sort files: `name`, `name_desc`, `date`, `date_desc`, `size`, `size_desc`, `extension`/`ext`, `extension_desc`/`ext_desc` (unknown values fall back to the `--numbering-source` order, else `name`) |

---

## Filtering

| Flag | Description |
|---|---|
| `--filter-mode <mode>` | Filter by type: `files`, `folders`, `both` (default: `both`) |
| `--min-name-len <n>` | Minimum filename length |
| `--max-name-len <n>` | Maximum filename length |
| `--min-path-len <n>` | Minimum full path length |
| `--max-path-len <n>` | Maximum full path length |
| `--filter-attr <attr>` | | Filter by attribute: `readonly`/`read-only`, `hidden`, `system`, `archive`, `file`, `dir`/`directory`/`folder` |
| `--filter-files` | Show only files (overrides `--filter-mode`) |
| `--filter-folders` | Show only folders (overrides `--filter-mode`) |
| `--filter-hidden` | Include hidden files |
| `--filter-subfolders` | Recurse into subfolders (same as `--recursive`) |
| `--filter-level <n>` | Maximum nesting level (used when `--max-depth` is not set) |
| `--filter-use-regex` | Treat the positional pattern as a regular expression when scanning recursively |
| `--filter-match-case` | Case-sensitive pattern matching when scanning recursively |

> **Note:** The file pattern is the positional `FILES` argument. `--filter-use-regex` and `--filter-match-case` control how that pattern is matched during a recursive scan; non-recursive globs are matched as globs.

> **Attribute filters** resolve through the same per-volume capability layer as
> `--set-attributes`. Filtering by an attribute a volume cannot express (e.g.
> `--filter-attr system` on ext4) matches nothing, because no file there has it.

---

## Presets

| Flag | Description |
|---|---|
| `--save-preset <path>` | Save current config as JSON preset |
| `--load-preset <path>` | Load config from JSON preset file |

> A preset is a shared **rename document**: `{"rename_config": {...}, "disabled_sections": ["AutoDate"]}`.
> Only non-default options are written. Disabled sections are honored on load — their
> values are cleared to defaults. The CLI always writes all sections enabled (it omits
> `disabled_sections`).

---

## Special Operations

| Flag | Description |
|---|---|
| `--gui` | Launch GUI window |
| `--tui` | Launch TUI (terminal UI) |
| `--swap` | Swap filenames between two files |
| `--theme <dark\|light\|system>` | GUI theme (used with `--gui`; default: `system`) |

---

## Examples

```bash
# Basic replace
awara "*.txt" --replace "old" --with "new"

# Add prefix with dry-run
awara --dry-run -r . --add-prefix "photo_"

# Regex rename
awara *.jpg --regex-match 'IMG_(\d+)' --regex-replace 'vacation_$1'

# Numbering with padding
awara *.png --numbering-mode prefix --numbering-pad 3

# Case conversion
awara "*.mp3" --case-name title --add-word-space

# Remove + trim + set case
awara . -r --remove-symbols --trim --case-name lower

# Save and load presets
awara *.txt --save-preset my-config.json
awara --load-preset my-config.json *.pdf

# Output to different directory
awara *.jpg --output-dir ./renamed --copy-mode

# Swap two filenames
awara file1.txt file2.txt --swap

# Set timestamps after rename
awara *.jpg --set-created taken --set-modified current

# Numbering by date, restart per folder
awara -r . --numbering-mode prefix --numbering-source date --numbering-restart-folder

# Crop before/after a match (one crop per run)
awara *.txt --crop before:prefix_
awara *.txt --crop after:_suffix
```
