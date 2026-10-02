# GUI Reference

The GUI (graphical user interface) provides a full desktop window with a visual,
point-and-click workflow for batch renaming. It's the best choice when you want
to browse directories, see a live preview of changes, and build rename operations
interactively through menus and form fields.

## Layout

The window is split into four main areas:

### Toolbar (top bar)

- **Refresh / Cancel** — re-scan the current directory or stop an in-progress scan
- **Revert** — open the revert dialog to roll back previous renames (per-session commit history)
- **About** — version info and interface overview
- **⚙ Settings** — app preferences and preset management

### Upper panel

- **Path bar** — shows the current directory path. Click the up-arrow to go to the parent,
  or type/paste a path and press Enter to jump there.
- **Tree navigator** (left sidebar) — browse the filesystem hierarchy. Click a folder to
  navigate there. Click the triangle to expand/collapse subtrees. Right-click for context
  actions (copy path, open in file manager, navigate here, refresh subtree, move to trash,
  properties).
- **File list / Preview** (center) — shows all files in the current directory with columns
  for Name, New Name, Type, Size, Modified, and Status. Click a column header to sort.
  Select files with mouse clicks (click to select, shift-click for range, Ctrl-click /
  ⌘-click to toggle a file in the selection).

### Lower panel — Rename Options

Build your rename pipeline by filling in the operation sections:

- **Replace** — find and replace text
- **RegEx** — pattern matching with capture groups
- **Remove** — remove by position, content type, or pattern
- **Add** — prefix, suffix, insert text, word spacing
- **Name** — set the filename stem (fixed, remove, reverse, pad numbers, reformat date)
- **Extension** — replace/append/remove extensions, extension case
- **Case** — case conversion for the name with exceptions
- **Numbering** — sequential numbering with base, padding, and reset rules
- **Move / Copy Parts** — relocate or duplicate a substring
- **Name Segment** — copy a character range from the name
- **Auto Date** — insert dates (all dates use **your local time zone** by default; the Off. field adds extra hours on top of local time)
- **Append Folder Name** — prepend or append parent directory name(s)
- **Filters** — mask (glob or regex), files/folders, hidden items, subfolders and nesting level, name/path length, match case, regex mode, and exclude pattern
- **Copy / Move to Location** — output directory, copy-not-move, keep structure. A folder moved to the location is always recreated empty there (its contents are not carried along and its source is kept); in-place folder renames are unaffected.

> Transfers to a location, and any rename whose computed name contains a path
> separator (which creates a subfolder and moves the entry into it), are
> structural relocations and are **not** recorded for undo/redo/revert. After
> applying, the moved entries leave the file pane (or keep their row, re-pointed,
> when **subfolders** is on) and the new folder is added to both the pane and the
> folder tree — no manual refresh needed.
>
> A rename may only *create subfolders* — it may not navigate the path. If a
> computed name is absolute/rooted or contains a `.` or `..` segment, the whole
> batch is invalid: **Apply** is blocked, nothing is renamed, and the offending
> names are listed. Because an apply that will create folders changes the layout,
> it asks for confirmation first (both **Apply** and an **F2** inline rename);
> tick **Don't ask again** (or the matching setting) to skip that prompt. This
> rule is enforced in the core, so the CLI and TUI refuse the whole batch too.
- **Special** — operation order, file attributes (⚙ Attributes), and post-rename timestamps (🕒 Timestamps). Attribute support depends on the target filesystem (see `docs/cli.md` for the per-volume table and the `hidden` dot-prefix rules).

At the top of this panel, use **⚡ Apply** to execute the rename and **Reset All** to
clear all options. Multiple operations are chained automatically in the configured
section order. If a rename target already exists on disk (or another operation in
the same batch claims it), a collision dialog appears before anything is changed,
letting you overwrite or skip each conflict. Folder handling applies whenever a
folder is involved on **either** side: an existing folder target cannot be
overwritten or merged, and a folder source cannot replace an existing file, so
**Replace** is disabled for both and the dialog shows a warning — such conflicts
are always skipped. Only a plain file → file collision can be replaced.

A file whose computed name equals its current name is never renamed (no
self-rename is attempted). It is still processed when an output directory is
configured (copied/moved) or when a requested attribute/timestamp can actually be
applied on the target filesystem; otherwise it is skipped (for example, setting
`system`/`archive` on a native ext4/APFS volume, or birth time on Linux).

The timestamp editor uses a 12-hour clock with **AM**/**PM** buttons. Values are
stored in canonical 24-hour local form (the same form the CLI/TUI and presets
use). An invalid date or time shows an inline ⚠ and disables **Confirm**, so an
unusable value can never be applied.

#### Operation Order

Sections run in a fixed default order: Name Segment → RegEx → Extension → Replace →
Name → Remove → Move / Copy Parts → Add → **Numbering** → Append Folder Name → Case →
Auto Date. Use the **▦ Order** button in the *Special* section to move a section up or
down. Operations always run in that configured order, so where numbering sits matters:
with `Add` prefix `A` and numbering prefix, Numbering before Add yields
`A1name.txt` while Numbering after Add yields `1Aname.txt`.

### Status bar (bottom)

Shows the current status message, plus counts for modified files, selected files,
and total objects in the current directory.

## Keyboard Shortcuts

### File list / Preview

| Key | Action |
|---|---|
| `↑` / `↓` | Move file selection up / down one row |
| `Shift+↑` / `Shift+↓` | Extend range selection up / down |
| `PageDown` | Jump selection down 25 rows |
| `PageUp` | Jump selection up 25 rows |
| `Home` | Jump to first file in list |
| `End` | Jump to last file in list |
| `Ctrl+A` / `⌘A` | Select all files |
| `Ctrl+D` / `⌘D` | Deselect all |
| `Ctrl+I` / `⌘I` | Invert selection |
| `Ctrl+Z` / `⌘Z` | Undo rename for all selected files |
| `Ctrl+Shift+Z` / `⌘Shift+Z` | Redo rename for all selected files |
| `Delete` | Move selected files to trash; while editing: delete character at cursor |
| `Enter` | Open selected directory / file with default app; while editing: commit and stay on current file |
| `F2` | Inline-edit selected filename (overrides rename operations for that file) |
| `Tab` / `Shift+Tab` | While editing: commit and move to next / previous file |
| `Esc` | Cancel inline edit / close open dialog |

> While an inline edit (`F2`) is active, keys that edit text belong to the
> filename field: `↑`/`↓` / `Home` / `End`, `Shift`+arrow selection,
> `Ctrl+Z` / `Ctrl+Shift+Z` (text undo/redo), `Ctrl+A`, and `Delete`. The file-list
> actions bound to those keys apply again once the edit is committed or cancelled.

### Navigation

| Key | Action |
|---|---|
| `Alt+↑` | Go to parent directory |
| `F5` | Refresh current directory scan |
| `Ctrl+F5` / `⌘F5` | Full refresh (re-scan + re-expand tree) |

### Revert Dialog

| Key | Action |
|---|---|
| `↑` / `↓` | Navigate entries up / down one row |
| `Shift+↑` / `Shift+↓` | Extend range selection up / down |
| `PageDown` | Jump down 25 entries |
| `PageUp` | Jump up 25 entries |
| `Home` | Jump to first entry |
| `End` | Jump to last entry |
| `Ctrl+A` / `⌘A` | Select all revertable entries |
| `Ctrl+D` / `⌘D` | Deselect all |
| `Ctrl+I` / `⌘I` | Invert selection |

## Launch Arguments

The GUI can be launched with optional arguments for theming and pre-seeding files:

```
awara --gui [theme] [files...]
```

> `--load-preset` is **not** recognized in this fast path (everything after an
> optional theme keyword is treated as a file path). To load a preset at launch, put
> another flag before `--gui` so the general flag parser is used, e.g.
> `awara --theme dark --gui --load-preset my-preset.json` — note that in this form
> positional files are ignored.

| Argument | Description |
|---|---|
| `--gui` | Force GUI mode (auto-detected if no arguments given) |
| `theme` | Optional first argument: one of `dark`, `light`, or `system` (default: `system`) |
| `files...` | One or more files/directories to open on launch |
| `--load-preset <path>` | Load a rename preset on startup (see note above) |

If a single directory is given, the GUI opens showing that directory. If files
are given, the GUI navigates to their parent directory and pre-selects them.

These arguments are designed for platform **context-menu integration** — the OS
passes selected files via `--gui` when the user right-clicks and selects "Send to"
or "Open with" on Windows, macOS, or Linux.

**Examples:**

```bash
# Launch with dark theme
awara --gui dark

# Launch showing a specific directory
awara --gui ~/Pictures

# Launch with files pre-selected (e.g. from context menu)
awara --gui light photo1.jpg photo2.jpg

# Launch with a preset loaded
awara --theme dark --gui --load-preset my-preset.json
```

> **Note:** The `--load-preset` flag also works in CLI mode. When loaded from the
> CLI, the preset is merged with a subset of additional CLI flags (prefix/suffix,
> regex, replace including `--replace-case-sensitive`, `--exclude-regex`, date
> inserts, numbering mode/source, case name, attributes, and timestamps); those flags
> override the preset. Other flags are ignored while a preset is loaded.

## Themes

Three theme modes are available:

| Mode | Description |
|---|---|
| `dark` | Dark background with light text (eyeball-friendly for low-light environments) |
| `light` | Light background with dark text |
| `system` | Follow the OS theme setting (default) |

The theme is set via the launch argument (`--gui dark`, or `--theme dark` when using
the general flag parser). It is not persisted between sessions.

## Settings

Open via the **⚙ Settings** button in the toolbar.

| Setting | Description |
|---|---|
| **Auto Refresh** | Automatically recompute the preview and re-apply filters when settings change |
| **Remember Last Directory** | Re-open the last-visited directory on next launch |
| **Remember Rename Options** | Persist the current rename configuration (all fields, timestamps, etc.) across sessions |
| **Skip trash confirmation** | Move files to the trash without a confirmation prompt |
| **Skip folder-creation warning** | Apply renames that create folders without the confirmation prompt |

### Presets

The Settings dialog also includes **Save / Load** buttons for rename presets.
Presets are shared rename documents — the rename configuration plus section enable
state — saved as JSON. Use them to quickly switch between common rename workflows.

- **Save** — exports current rename options as a `.json` file
- **Load** — imports a previously saved preset and applies it to the current session (including its section enable state)

> Presets are compatible between GUI, CLI, and TUI — save in one interface and
> load in another.

## Persistence

The GUI saves state to the platform config directory:

| Platform | Config Path |
|---|---|
| Linux | `~/.config/awara/` |
| macOS | `~/Library/Application Support/Awara/` |
| Windows | `%APPDATA%\Awara\` |

| File | Description |
|---|---|
| `app_settings.json` | General preferences (auto-refresh, remember-last-dir, remember-rename-options, last directory, skip trash confirmation) plus UI view state (preview-table column widths and order, revert-dialog column widths, preview sort column and direction) |
| `rename_options.json` | Saved rename document (only written when "Remember Rename Options" is enabled): the shared `rename_config` + `disabled_sections`, plus GUI-only editor state |

To reset the GUI to defaults, delete one or both files from the config directory.

> `rename_options.json` is the shared rename document (the same format as presets)
> with the GUI's editor state added; see `docs/cli.md` for the preset format.
