use super::*;
use crate::core::schema::{ATTR_ARCHIVE, ATTR_HIDDEN, ATTR_SYSTEM};

/// Bit flags for [`RenameOp::applied_timestamps_mask`] — the timestamps an
/// operation intentionally set. Single source of truth for the bit meanings.
pub const TS_CREATED: u8 = 0b001;
pub const TS_MODIFIED: u8 = 0b010;
pub const TS_ACCESSED: u8 = 0b100;

/// Serializable snapshot of file metadata captured before rename/copy.
/// Stored on RenameOp so it can be restored on revert.
///
/// The cross-platform fields (`readonly`, mtime, atime) are always captured.
/// The optional fields record prior values **only for the categories the op
/// intentionally changes** (see [`PermSnapshot::capture_for`]), so a restore
/// never touches metadata the operation didn't mean to change.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PermSnapshot {
    /// Whether the file was readonly
    pub readonly: bool,
    /// Pre-rename modification time (UNIX epoch seconds)
    pub mtime_secs: i64,
    /// Nanoseconds part of mtime
    pub mtime_nanos: u32,
    /// Pre-rename access time (UNIX epoch seconds)
    pub atime_secs: i64,
    /// Nanoseconds part of atime
    pub atime_nanos: u32,
    /// Prior hidden bit, captured only when the op applies `hidden` and the
    /// volume exposes it as an in-place flag (a DOS hidden bit or macOS
    /// `UF_HIDDEN`). `None` for the rename-based dot-prefix convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// Prior system bit (DOS-bit volumes; captured only when applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<bool>,
    /// Prior archive bit (DOS-bit volumes; captured only when applied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<bool>,
    /// Prior creation (birth) time as `(secs, nanos)`, captured only when the
    /// op sets `created` and the platform can restore it (Windows, macOS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<(i64, u32)>,
}

/// True when `applied` (comma-separated `[+|-]name` tokens) names `attr`.
fn attr_applied(applied: &str, attr: &str) -> bool {
    applied.split(',').any(|t| {
        t.trim()
            .trim_start_matches(['+', '-'])
            .trim()
            .eq_ignore_ascii_case(attr)
    })
}

impl PermSnapshot {
    /// Capture metadata from a path. Returns None if metadata is unavailable.
    pub fn capture(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Self::from_metadata(&meta)
    }

    /// Base snapshot from already-fetched metadata (so callers that also need
    /// other metadata don't pay for a second syscall).
    fn from_metadata(meta: &fs::Metadata) -> Option<Self> {
        let readonly = meta.permissions().readonly();
        let modified = meta.modified().ok()?;
        let accessed = meta.accessed().ok()?;
        let md = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        let ad = accessed.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(PermSnapshot {
            readonly,
            mtime_secs: md.as_secs() as i64,
            mtime_nanos: md.subsec_nanos(),
            atime_secs: ad.as_secs() as i64,
            atime_nanos: ad.subsec_nanos(),
            hidden: None,
            system: None,
            archive: None,
            created: None,
        })
    }

    /// Capture prior metadata, additionally recording the platform attribute
    /// bits and the creation time **only for the categories the op intends to
    /// change** (`applied_attributes` tokens and `applied_timestamps_mask`).
    /// Reads metadata once.
    pub fn capture_for(
        path: &Path,
        applied_attributes: &str,
        applied_timestamps_mask: u8,
    ) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        let mut snap = Self::from_metadata(&meta)?;
        // Only in-place metadata flags are captured as a value to restore;
        // dot-prefix `hidden` lives in the file name and is undone by the
        // rename itself.
        let caps = crate::core::attrs::caps_for_read(path);
        if attr_applied(applied_attributes, ATTR_HIDDEN) && caps.inplace_metadata(ATTR_HIDDEN) {
            snap.hidden = caps.hidden.get(path, Some(&meta));
        }
        if attr_applied(applied_attributes, ATTR_SYSTEM) && caps.inplace_metadata(ATTR_SYSTEM) {
            snap.system = caps.system.get(path, Some(&meta));
        }
        if attr_applied(applied_attributes, ATTR_ARCHIVE) && caps.inplace_metadata(ATTR_ARCHIVE) {
            snap.archive = caps.archive.get(path, Some(&meta));
        }
        if applied_timestamps_mask & TS_CREATED != 0 && caps.birth_time {
            snap.created = crate::core::platform::creation_time_parts(&meta);
        }
        Some(snap)
    }

    /// Restore permissions and timestamps to a path.
    ///
    /// This is the single restore path: it brings back everything the snapshot
    /// captured — the base metadata plus any extras recorded by
    /// [`PermSnapshot::capture_for`]. "Only remember intentional changes" lives
    /// in capture, not here: whatever was deliberately captured is restored.
    pub fn apply(&self, path: &Path) {
        self.restore_readonly(path);
        let _ = filetime::set_file_times(
            path,
            filetime::FileTime::from_system_time(self.atime()),
            filetime::FileTime::from_system_time(self.mtime()),
        );
        self.apply_intentional_extras(path);
    }

    fn mtime(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::new(self.mtime_secs as u64, self.mtime_nanos)
    }

    fn atime(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::new(self.atime_secs as u64, self.atime_nanos)
    }

    fn restore_readonly(&self, path: &Path) {
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_readonly(self.readonly);
            let _ = fs::set_permissions(path, perms);
        }
    }

    /// Restore the platform attribute bits / creation time captured by
    /// [`PermSnapshot::capture_for`] (all `Option`s are only `Some` when the op
    /// intentionally changed that category).
    fn apply_intentional_extras(&self, path: &Path) {
        let caps = crate::core::attrs::caps_for_write(path);
        if let Some(hidden) = self.hidden
            && caps.inplace_metadata(ATTR_HIDDEN)
        {
            caps.set(path, ATTR_HIDDEN, hidden);
        }
        if let Some(system) = self.system
            && caps.inplace_metadata(ATTR_SYSTEM)
        {
            caps.set(path, ATTR_SYSTEM, system);
        }
        if let Some(archive) = self.archive
            && caps.inplace_metadata(ATTR_ARCHIVE)
        {
            caps.set(path, ATTR_ARCHIVE, archive);
        }
        if let Some((secs, nanos)) = self.created
            && caps.birth_time
        {
            let t = std::time::UNIX_EPOCH + std::time::Duration::new(secs as u64, nanos);
            let _ = crate::core::platform::set_creation_time(path, t);
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RenameOp {
    pub original_path: PathBuf,
    pub new_path: PathBuf,
    pub original_name: String,
    pub new_name: String,
    /// True if this operation was a copy (source file still exists after execution).
    #[serde(default)]
    pub was_copy: bool,
    /// True when this op's target was relocated by the Copy / Move to Location
    /// (output-directory) transform. The planner records this in
    /// `apply_output_dir`; it is never inferred from path shape (a computed name
    /// containing a separator must not read as a relocation). This is the single
    /// source of truth for two decisions: the op is not undoable, and a
    /// **directory** it relocates is copied (recreated empty) rather than moved.
    #[serde(default)]
    pub relocated: bool,
    /// Snapshot of pre-rename file metadata for revert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_permissions: Option<PermSnapshot>,
    /// The attribute string that was applied (e.g. "readonly,hidden"). Empty = no change.
    #[serde(default)]
    pub applied_attributes: String,
    /// Bitmask of timestamp operations applied: bit 0=created, 1=modified, 2=accessed
    #[serde(default)]
    pub applied_timestamps_mask: u8,
}

impl RenameOp {
    /// True when applying this op would not change the path on disk.
    ///
    /// This is the single definition of a no-op rename. Exact path equality
    /// (not case-insensitive `is_same_file`): a case-only rename (`a.txt` →
    /// `A.txt`) and an output-directory move (`a.txt` → `out/a.txt`) both
    /// change the path and remain actionable.
    #[inline]
    pub fn is_noop(&self) -> bool {
        self.new_path == self.original_path
    }

    /// True when this op's target lands in a different directory than its
    /// source — i.e. the entry moved to another directory **on disk**.
    ///
    /// This is a filesystem fact used for IO and cache maintenance (create the
    /// target directory, invalidate the tree, decide whether a row left the
    /// listing). It is deliberately **not** a classification: which ops are
    /// location transfers or path-producing renames is recorded explicitly in
    /// [`RenameOp::relocated`] and [`RenameOp::name_implies_subpath`], never
    /// re-derived from path shape.
    #[inline]
    pub fn lands_in_other_directory(&self) -> bool {
        self.new_path.parent() != self.original_path.parent()
    }

    /// True when this op relocated its target to a different directory because of
    /// the Copy / Move to Location (output-directory) transform — recorded
    /// explicitly by the planner, never inferred here from path shape. Such
    /// transfers are not recorded for undo/redo/revert.
    #[inline]
    pub fn is_location_transfer(&self) -> bool {
        self.relocated
    }

    /// True when the computed name itself produced a path — it carried a path
    /// separator, so the op moves the entry into a subpath of its current folder.
    ///
    /// This is deliberately **separate** from [`RenameOp::relocated`]: it comes
    /// from the name pipeline (including an F2 inline edit), not from the Copy /
    /// Move to Location transform, and it **moves** the entry — a directory keeps
    /// its children (see `op_is_copy`). It is, however, a structural relocation
    /// like a location transfer, so it is excluded from undo/redo/revert
    /// ([`RenameOp::is_undoable`]).
    #[inline]
    pub fn name_implies_subpath(&self) -> bool {
        self.new_name.chars().any(std::path::is_separator)
    }

    /// True when this op should be recorded for undo/redo/revert.
    ///
    /// An op that moved the entry to a different directory — a Copy / Move to
    /// Location relocation (`relocated`) or a path-producing rename
    /// ([`RenameOp::name_implies_subpath`]) — is a structural transfer, not a
    /// plain rename, so it is not undoable; every other op is. This is the single
    /// rule shared by the GUI commit history and the CLI/TUI undo file.
    ///
    /// Both signals are explicit facts on the op, so this needs no plan-level
    /// state and never has to guess from path shape.
    #[inline]
    pub fn is_undoable(&self) -> bool {
        !self.relocated && !self.name_implies_subpath()
    }
}

/// Strategy for handling collisions where target already exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionStrategy {
    Skip,
    Overwrite,
}

/// One collision detected during pre-scan.
#[derive(Debug, Clone)]
pub struct Collision {
    pub op_index: usize,
    pub reason: CollisionReason,
    /// True when the colliding target is an existing directory on disk. Folders
    /// cannot be replaced or merged, so `Overwrite` refuses them (skips); this
    /// flag lets every frontend report that without re-statting the path.
    /// Uses no-follow semantics (`symlink_metadata`) to match `fs::rename`.
    pub target_is_dir: bool,
    /// True when the *source* is an existing directory (no-follow). A folder is
    /// never allowed to replace an existing target either, so `Overwrite` skips
    /// whenever a folder is involved on **either** side — see
    /// [`Collision::folder_involved`].
    pub source_is_dir: bool,
}

impl Collision {
    /// True when a folder is involved on either side of the collision. Folder
    /// handling (skip, never replace or merge) applies to all of these.
    pub fn folder_involved(&self) -> bool {
        self.target_is_dir || self.source_is_dir
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollisionReason {
    /// Target path already exists on disk (and is NOT another op's source).
    OnDisk,
    /// Another op in the same batch targets the same path.
    InBatch(usize),
}

/// A lightweight cancellation token shared between signal handlers and the rename loop.
pub type CancellationToken = std::sync::Arc<std::sync::atomic::AtomicBool>;

/// Create a new cancellation token (returns the token and a clone for the handler).
pub fn cancel_token() -> (CancellationToken, CancellationToken) {
    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let clone = std::sync::Arc::clone(&token);
    (token, clone)
}

/// Events emitted during the rename pipeline.
#[derive(Debug, Clone)]
pub enum RenameEvent {
    Progress {
        original: String,
        new: String,
    },
    Collision {
        original: String,
        new: String,
    },
    Error {
        original: String,
        new: String,
        message: String,
    },
    Skipped {
        original: String,
        reason: String,
    },
    Done {
        success: usize,
        errors: usize,
        skipped: usize,
    },
    RollbackProgress {
        original: String,
        new: String,
    },
    Status {
        message: String,
    },
    /// Pipeline was cancelled mid-way (user sent SIGINT/SIGTERM).
    /// The result will contain whatever operations completed before cancellation.
    Cancelled {
        completed: usize,
    },
}

/// Options controlling rename execution behavior.
///
/// Planning inputs live in [`PlanOptions`](crate::PlanOptions) — the computed
/// [`RenamePlan`](crate::RenamePlan) already carries the target paths, so
/// execution only decides how (and whether) to apply them. The collision policy
/// is passed to [`execute_plan`](crate::execute_plan) as a
/// [`Resolution`](crate::Resolution).
pub struct RenameOptions<'a> {
    pub dry_run: bool,
    pub stop_on_error: bool,
    pub preserve_timestamps: bool,
    pub set_attributes: Option<&'a str>,
    pub undo_file: Option<&'a str>,
    pub on_event: Option<Box<dyn FnMut(RenameEvent) + 'a>>,
    pub cancel_token: Option<CancellationToken>,
}

impl<'a> Default for RenameOptions<'a> {
    fn default() -> Self {
        Self {
            dry_run: false,
            stop_on_error: true,
            preserve_timestamps: false,
            set_attributes: None,
            undo_file: None,
            on_event: None,
            cancel_token: None,
        }
    }
}

/// Outcome of the rename pipeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameResult {
    pub successful_ops: Vec<RenameOp>,
    pub failed_ops: Vec<(RenameOp, String)>,
    pub skipped_ops: Vec<(RenameOp, String)>,
}

/// Result of executing a batch of explicit rename ops chain-safely.
pub struct OpBatchResult {
    /// Ops executed on disk (original_path → new_path), in execution order.
    pub successful: Vec<RenameOp>,
    /// Ops that could not be executed, with the error reason.
    pub failed: Vec<(RenameOp, String)>,
}
