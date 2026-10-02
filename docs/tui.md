# TUI Reference

The TUI (terminal UI) is an interactive, keyboard-driven interface for environments
where a desktop GUI isn't available or desired. It's organized into four panes:

- **Navigator** — browse and select files
- **Preview** — see rename results before applying
- **Config** — review, reorder, and manage your rename operations
- **Console** — enter commands, toggle modes, execute renames

Bindings are either **keyboard shortcuts** (navigation and common actions) or
**console commands** (typed in the Console to build the rename pipeline).

---

## Usage

1. **Navigate** to the target directory (`Tab` to focus Navigator, arrow keys to move,
   `Enter` to open a folder).
2. **Select** files with `Space`, or use `a` to select all / `A` to deselect all.
3. **Build your rename pipeline** by typing operations into the Console (e.g.
   `--replace "foo" --with "bar"`); they are staged as items in the Config pane.
   Use `S` in Config to toggle stacking mode for multi-step pipelines.
4. **Preview** results live in the Preview pane.
5. **Apply** with `apply` in the Console, or toggle dry-run mode with `D` in Config
   to test safely first.

---

## Global (any pane)

| Key | Action |
|---|---|
| `Tab` | Cycle focus forward (Navigator → Preview → Config → Console) |
| `Shift+Tab` | Cycle focus backward (Console → Config → Preview → Navigator) |
| `Ctrl+C` | Quit from anywhere |
| `?` | Toggle help overlay |
| `Esc` | Close help overlay / cancel scan / cancel reorder |

In the help overlay (`?`), `↑`/`↓` or `j`/`k` move the selection, `F` cycles the
category filter, `Esc` closes it, and `q` quits the app.

---

## Navigator

| Key | Action |
|---|---|
| `↓` / `j` | Move selection down |
| `↑` / `k` | Move selection up |
| `Shift+↓` | Move down and add to selection |
| `Shift+↑` | Move up and add to selection |
| `Enter` | Enter selected directory |
| `Backspace` | Go up to parent directory |
| `Space` | Toggle selection on current item |
| `a` | Select all visible files |
| `A` | Deselect all |
| `r` | Refresh navigator (re-scan directory) |
| `R` | Toggle recursive mode |
| `H` | Toggle show hidden files |
| `s` | Cycle sort mode incl. direction (Name → NameDesc → Date → DateDesc → Size → SizeDesc → Extension → ExtensionDesc → Name) |
| `S` | Toggle sort direction (Asc/Desc) |
| `F` | Cycle filter mode (Files → Folders → Both) |
| `q` | Quit |
| `Esc` | Cancel an in-progress scan |

---

## Preview

| Key | Action |
|---|---|
| `↓` / `j` | Move preview selection down |
| `↑` / `k` | Move preview selection up |
| `s` | Cycle sort mode (same cycle as Navigator, incl. direction) |
| `S` | Toggle sort direction |

---

## Config

| Key | Action |
|---|---|
| `↓` / `j` | Move selection down / move dragged item down |
| `↑` / `k` | Move selection up / move dragged item up |
| `Space` | Start / cancel reorder drag |
| `Enter` | Confirm reorder position / inline-edit selected config item (opens Console) |
| `Esc` | Cancel reorder / exit Config mode |
| `d` / `Delete` | Remove selected config item |
| `D` | Toggle dry-run mode |
| `S` | Toggle stacking mode ON/OFF |

---

## Console

| Key | Action |
|---|---|
| `Enter` | Execute command / save inline edit |
| `Esc` | Clear input & return to Navigator / cancel inline edit |
| `↑` | Navigate history backward |
| `↓` | Navigate history forward |
| `←` / `→` | Move cursor left / right |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character at cursor |
| `Tab` | Tab-complete directories (when typing `cd <partial>`) |

### Console Commands

#### Navigation

| Command | Description |
|---|---|
| `cd <path>` | Change directory (`~` supported) |
| `filter [pat]` | Set / clear glob filter pattern |
| `sort <mode>` | Set sort mode: `name`, `date`, `size`, or their `-desc` forms |
| `reset` | Reset all config to an empty state |

#### Rename Operations (single mode — replaces current config)

| Command | Description |
|---|---|
| `--add-prefix <s>` | Add prefix |
| `--add-suffix <s>` | Add suffix |
| `--replace <f> --with <r>` | Find & replace |
| `--replace-first` | Replace only the first occurrence |
| `--regex-match <p> --regex-replace <r>` | Regex replace |
| `--remove-first <n>` | Remove first n chars |
| `--remove-last <n>` | Remove last n chars |
| `--remove-from <n> --remove-to <n>` | Remove char range |
| `--remove-digits` | Remove all digits |
| `--remove-chars <s>` | Remove specified chars |
| `--remove-words <s>` | Remove specified words |
| `--remove-symbols` | Remove symbols |
| `--remove-all-chars` | Remove all characters (leaves extension) |
| `--trim` | Trim whitespace |
| `--double-spaces` | Collapse runs of whitespace to a single space |
| `--remove-lead-dots <mode>` | Remove leading dots (single, double, both) |
| `--crop <spec>` | Crop text: before:TEXT, after:TEXT, or special:TEXT |
| `--case-name <mode>` | Change case (lower, upper, title, title-enhanced, sentence, invert) |
| `--case-exception <word>` | Preserve word as-is during case conversion |
| `--numbering-mode <m>` | Set numbering mode (prefix, suffix, both, insert) |
| `--numbering-start <n>` | Starting number |
| `--numbering-increment <n>` | Increment |
| `--numbering-pad <n>` | Zero-padding width |
| `--numbering-sep <s>` | Number separator |
| `--numbering-at <n>` | Insert at position |
| `--numbering-break <n>` | Reset every N files |
| `--numbering-restart-folder` | Restart per folder |
| `--numbering-type <type>` | Numbering base/type (decimal, hex, octal, az_upper, az_lower, alpha, roman, 2–36) |
| `--numbering-case <c>` | Letter case (lower, upper) |
| `--numbering-source <s>` | Sort source for numbering (name, date, size) |
| `--extension <ext>` | Set extension |
| `--extension-add <s>` | Append to extension |
| `--extension-mode <m>` | Extension case (lower, upper, title) |
| `--extension-remove` | Remove extension |
| `--extension-add-if-missing` | Only add extension if none present *(not yet honored by the engine — the extension is written unconditionally)* |
| `--add-insert <s> --add-at <n>` | Insert text at position (negative counts from the end) |
| `--add-word-space` | Add space between words |
| `--add-dirname` | Insert directory name |
| `--dirname-sep <s>` | Dirname separator |
| `--dirname-pos <n>` | Dirname position |
| `--dirname-level <n>` | Number of parent folders to add (1 = immediate parent; higher levels add ancestors, ordered grandparent → parent) |
| `--move-part <s:l:to>` | Move substring |
| `--copy-part <s:l:to>` | Copy substring |
| `--add-date <fmt>` | Insert current date (prefix by default) |
| `--add-file-date <fmt>` | Insert file modification date (prefix by default) |
| `--insert-meta <tag>` | Insert metadata tag (prefix by default), e.g. exif-date |
| `--date-position <pos>` | Date position: `none`, `prefix`, or `suffix` (default: `none`, or `prefix` when a date field is set) |
| `--auto-date-century` | Include the century (YYYY) in auto-date output |
| `--auto-date-offset <hours>` | Add extra hours to auto-date |
| `--auto-date-sep-filename <s>` | Separator between filename and auto-date |
| `--exclude-regex <p>` | Exclude files matching regex |
| `--name-mode <mode>` | Set name mode (keep, fixed, remove, reverse, pad_numbers, reformat_date) |
| `--name-value <value>` | Fixed name value |
| `--name-segment-from <n> --name-segment-to <n>` | Copy name segment |
| `--regex-simple <pattern>` | Simplified regex mode |
| `--regex-full-name` | Apply regex to full name (inc. extension) |
| `--replace-case-sensitive` | Case-sensitive matching for replace and remove-words (regex is always case-sensitive; use an inline (?i)) |
| `--preserve-timestamps` | Preserve file timestamps |
| `--output-dir <dir>` | Copy/move renamed files to this directory |
| `--copy-mode` | Copy instead of move (default when `--output-dir` is set). Directories are recreated empty — their contents are not copied |
| `--move` | Move instead of copy. A relocated directory is still recreated empty (its contents are not moved; the source folder is kept) |
| `--keep-structure` | Keep directory structure when copying to output-dir |
| `--set-attributes <a>` | Set file attributes (readonly, hidden, system, archive; support depends on the target filesystem) |
| `--set-created <spec>` | Set creation time (`current`, `taken`, `delta:<±secs>`, `copy-from:created\|modified\|accessed`, or a local datetime `YYYY-MM-DD[ HH:MM[:SS]][ AM\|PM]`) |
| `--set-modified <spec>` | Set modification time (same syntax) |
| `--set-accessed <spec>` | Set access time (same syntax) |
| `--filter-attr <attr>` | Filter by attribute (`readonly`/`read-only`, `hidden`, `system`, `archive`, `file`, `dir`/`directory`/`folder`) |
| `--filter-mode <mode>` | Filter by type (files, folders, both) |
| `--filter-files` | Filter files only |
| `--filter-folders` | Filter folders only |
| `--filter-hidden` | Include hidden files |
| `--filter-subfolders` | Include subfolders |
| `--recursive` | Include subfolders (same as `--filter-subfolders`) |
| `--filter-level <n>` | Max nesting level |
| `--filter-use-regex` | Regex filter patterns |
| `--filter-match-case` | Case-sensitive filter |
| `--min-name-len <n>` | Min filename length |
| `--max-name-len <n>` | Max filename length |
| `--min-path-len <n>` | Min path length |
| `--max-path-len <n>` | Max path length |
| `--stop-on-error` | Stop on error |
| `--remove <mode>` | Extended remove (digits, high, trim, double-spaces, accents, symbols) |

> ⏰ **Timestamps:** values (syntax in the table above) are interpreted in **your local time zone** and converted to UTC when written to the filesystem; invalid values are rejected with an error rather than silently ignored. `--set-created` is settable only where the platform/volume exposes a birth time (Windows, macOS) and is a no-op on Linux. See `docs/cli.md` for details.

> 📁 **Attributes:** `--set-attributes` accepts `name`/`+name` to set and `-name`
> to clear (e.g. `readonly,-hidden`); support is per-volume, not per-OS. Unsupported
> attributes are skipped and reported once, and a file whose name does not change is
> processed only when a requested effect applies to the target volume. See `docs/cli.md`
> for the per-volume table, including `hidden`'s leading-dot fallback on native Unix.

#### Rename Operations (stacking mode — appends to command list)

When stacking mode is on (`stacking` command), each command appends to a list of
operations that execute in order. You can chain multiple commands together, including
the same operation multiple times (e.g. two separate `--add-prefix` steps).

Each command is parsed independently from scratch, so sub-options (like
`--replace-case-sensitive`) must be on the same line as the operation they modify —
restate them in every stacked command that needs them:

```
# correct: the sub-option is on the same line as its operation
--replace foo --with bar --replace-case-sensitive

# wrong: the 2nd command has no --replace, so the flag is lost
--replace foo --with bar
--replace-case-sensitive
```

For stacking mode, `reset` clears the entire command list.

Non-operation flags (`--stop-on-error`, `--output-dir`, `--copy-mode`,
`--preserve-timestamps`, `--set-attributes`, filters, etc.) do not produce rename
items, so they are silently ignored in stacking mode. Toggling stacking **ON** rebuilds
the config from the staged operations and preserves only `--output-dir`, `--copy-mode`,
`--stop-on-error`, and the min/max name/path-length filters; every other non-operation
flag set beforehand is discarded. To use those, stay in normal mode (stacking OFF).

#### Execution & Config Management

| Command | Description |
|---|---|
| `preview` | Recompute preview |
| `apply` / `proceed` | Execute the renames |
| `apply --overwrite` / `proceed --overwrite` | Execute and overwrite all collisions |
| `apply --skip` / `proceed --skip` | Execute and skip all collisions |
| `stacking` | Toggle stacking mode (multiple commands) |
| `dry-run` / `dry_run` | Toggle dry-run mode |
| `save-preset <path>` | Save config as JSON preset |
| `load-preset <path>` | Load config from JSON preset |
| `undo-file [path]` | Write a JSON undo record of successful in-place renames to `[path]` (no path disables it). Copy / Move to Location transfers, and renames whose new name contains a path separator (which create a subfolder), are not recorded |
| `help` / `--help` / `h` | Show help overlay |
| `exit` / `quit` | Quit the application |

When collisions are detected, `proceed` (without a flag) does not execute — it
prompts you to re-run with `proceed --overwrite` or `proceed --skip`, mirroring
the GUI's collision dialog. Folder handling applies whenever a folder is
involved on **either** side — an existing folder target, or a folder source over
an existing file — so `proceed --overwrite` skips those. Only a plain file → file
collision is overwritten.

> Presets use the shared rename **document** format (`rename_config` plus an optional
> `disabled_sections` list). Disabled sections are honored on load — their values are
> cleared to defaults — and the TUI always writes all sections enabled.
