use crate::args::Args;
use awara::{
    CollisionStrategy, EntryFilter, NumberingSource, RenameEvent, RenameOptions, ScanOptions,
    ScanSortBy, filter_path, scan_directory, sort_paths,
};
use clap::CommandFactory;
use glob::glob;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Runs the batch-rename pipeline: validate paths, expand globs, deduplicate,
/// sort, and execute the rename operations.
pub fn run(args: Args) {
    // ── Early path validation (user-typed paths, not glob-discovered) ──

    // Validate --apply-undo path
    if let Some(undo_path) = &args.apply_undo {
        match awara::validate_input_path(undo_path) {
            awara::InputPathValidation::Ok(_) => {}
            awara::InputPathValidation::Empty => {
                eprintln!("Error: undo file path is empty");
                std::process::exit(1);
            }
            awara::InputPathValidation::NotFound => {
                eprintln!("Error: undo file '{}' not found", undo_path);
                std::process::exit(1);
            }
            awara::InputPathValidation::NotAccessible(e) => {
                eprintln!("Error: cannot read undo file '{}': {}", undo_path, e);
                std::process::exit(1);
            }
            awara::InputPathValidation::PathTooLong { length, limit } => {
                eprintln!(
                    "Error: undo file path is {} characters (limit is {})",
                    length, limit
                );
                std::process::exit(1);
            }
            awara::InputPathValidation::ComponentTooLong {
                component,
                length,
                limit,
            } => {
                eprintln!(
                    "Error: undo file path component '{}' is {} bytes (limit is {})",
                    component, length, limit
                );
                std::process::exit(1);
            }
            awara::InputPathValidation::TrailingDotOrSpace => {
                eprintln!(
                    "Error: undo file path '{}' ends with a space or dot (not supported)",
                    undo_path
                );
                std::process::exit(1);
            }
            other => {
                eprintln!("Error: invalid undo file path '{}': {:?}", undo_path, other);
                std::process::exit(1);
            }
        }
        match std::fs::read_to_string(undo_path) {
            Ok(data) => match serde_json::from_str::<Vec<awara::RenameOp>>(&data) {
                Ok(ops) => {
                    let (reverted, errors) = awara::apply_undo(&ops);
                    println!(
                        "Undo complete: {} reverted, {} errors",
                        reverted.len(),
                        errors
                    );
                    if errors > 0 {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to parse undo file '{}': {}", undo_path, e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("Failed to read undo file '{}': {}", undo_path, e);
                std::process::exit(1);
            }
        }
        return;
    }

    if args.files.is_empty() {
        let _ = Args::command().print_help();
        std::process::exit(1);
    }

    // Validate --output-dir if provided
    // Result is discarded — structural validation (length, chars) is sufficient;
    // the directory will be created at rename time if it doesn't exist.
    if let Some(out_dir) = &args.output_dir {
        let _ = awara::validate_input_path(out_dir);
    }

    // Validate --save-preset path
    if let Some(path) = &args.save_preset
        && let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        eprintln!(
            "Error: parent directory for preset '{}' does not exist",
            path
        );
        std::process::exit(1);
    }

    // Validate --load-preset path and content
    if let Some(path) = &args.load_preset {
        match awara::validate_input_path(path) {
            awara::InputPathValidation::Ok(_) => {}
            awara::InputPathValidation::Empty => {
                eprintln!("Error: preset path is empty");
                std::process::exit(1);
            }
            awara::InputPathValidation::NotFound => {
                eprintln!("Error: preset file '{}' not found", path);
                std::process::exit(1);
            }
            other => {
                eprintln!("Error: invalid preset path '{}': {:?}", path, other);
                std::process::exit(1);
            }
        }
        // Validate content can be parsed as a RenameConfig and is semantically valid
        match std::fs::read_to_string(path) {
            Ok(data) => match awara::RenameDoc::from_json_with_warnings(&data) {
                Ok(loaded) => {
                    for warning in &loaded.warnings {
                        eprintln!("Warning: preset '{}': {}", path, warning);
                    }
                }
                Err(e) => {
                    eprintln!("Error: preset file '{}' is not valid: {}", path, e);
                    std::process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("Error: could not read preset file '{}': {}", path, e);
                std::process::exit(1);
            }
        }
    }

    // Handle swap mode
    if args.swap {
        return swap_files(&args);
    }

    let config = args.clone().into_config();

    // Handle save_preset
    if let Some(preset_path) = &args.save_preset
        && let Ok(json) = awara::RenameDoc::new(config.clone()).to_json_pretty()
    {
        if std::fs::write(preset_path, &json).is_ok() {
            eprintln!("Preset saved to {}", preset_path);
        } else {
            eprintln!("Failed to save preset to {}", preset_path);
        }
    }

    // Shared scan options for file discovery (SSoT with TUI).
    // `--filter-files`/`--filter-folders` override `--filter-mode`; otherwise the
    // mode is used as-is (default: both).
    let entry_filter = match (args.filter_files, args.filter_folders) {
        (true, false) => EntryFilter::Files,
        (false, true) => EntryFilter::Folders,
        (true, true) => EntryFilter::Both,
        (false, false) => args.filter_mode.unwrap_or(EntryFilter::Both),
    };
    // `--filter-subfolders` implies recursion, and `--filter-level` acts as the
    // maximum nesting depth when `--max-depth` is not given.
    let recursive = args.recursive || args.filter_subfolders;
    let max_depth = args.max_depth.or(args.filter_level);
    let scan_opts = ScanOptions {
        recursive,
        max_depth,
        show_hidden: args.filter_hidden,
        entry_filter,
        filter_pattern: None,
        exclude_regex: args.exclude_regex.clone(),
        min_name_len: args.min_name_len,
        max_name_len: args.max_name_len,
        min_path_len: args.min_path_len,
        max_path_len: args.max_path_len,
        filter_attr: args.filter_attr.clone(),
        filter_use_regex: args.filter_use_regex,
        filter_match_case: args.filter_match_case,
        sort_by: None, // We sort separately after numbering_source
    };

    // Expand globs / Walk directories using shared scan_directory
    let mut files: Vec<PathBuf> = Vec::new();
    for pattern in args.files {
        if recursive {
            let path = Path::new(&pattern);
            if path.is_dir() {
                println!("Walking dir: {:?}", path);
                let dir_files = scan_directory(path, &scan_opts, None);
                files.extend(dir_files);
            } else {
                // pattern is a glob, walk from "." matching the pattern
                let mut glob_opts = scan_opts.clone();
                glob_opts.filter_pattern = Some(pattern.clone());
                files.extend(scan_directory(Path::new("."), &glob_opts, None));
            }
        } else {
            // Distinguish literal paths from glob patterns for better error messages
            let has_glob_chars = pattern.contains('*') || pattern.contains('?');

            if !has_glob_chars {
                // Literal path — validate it exists before globbing
                match awara::validate_input_path(&pattern) {
                    awara::InputPathValidation::Ok(_) => {}
                    awara::InputPathValidation::Empty => {
                        eprintln!("Error: file path is empty");
                        std::process::exit(1);
                    }
                    awara::InputPathValidation::NotFound => {
                        eprintln!("Error: path '{}' does not exist", pattern);
                        std::process::exit(1);
                    }
                    awara::InputPathValidation::NotAccessible(e) => {
                        eprintln!("Error: cannot access '{}': {}", pattern, e);
                        std::process::exit(1);
                    }
                    awara::InputPathValidation::ComponentTooLong {
                        component,
                        length,
                        limit,
                    } => {
                        eprintln!(
                            "Error: path component '{}' is {} bytes (limit is {})",
                            component, length, limit
                        );
                        std::process::exit(1);
                    }
                    awara::InputPathValidation::TrailingDotOrSpace => {
                        eprintln!(
                            "Error: path '{}' ends with a space or dot (not supported on Windows)",
                            pattern
                        );
                        std::process::exit(1);
                    }
                    other => {
                        eprintln!("Error: invalid path '{}': {:?}", pattern, other);
                        std::process::exit(1);
                    }
                }
            }

            match glob(&pattern) {
                Ok(paths) => {
                    let mut found = false;
                    for path in paths.flatten() {
                        if filter_path(&path, &scan_opts) {
                            files.push(path);
                            found = true;
                        }
                    }
                    if !found && has_glob_chars {
                        eprintln!("Warning: No files matched glob '{}'", pattern);
                    }
                    // Literal paths that passed validation above should always match
                }
                Err(e) => {
                    eprintln!("Warning: Invalid glob pattern '{}': {}", pattern, e);
                }
            }
        }
    }

    // ── Deduplicate paths that refer to the same file ──
    // Normalize . and .. lexically (without following symlinks) and dedup.
    // This ensures `./foo/../bar/file` and `bar/file` are not both processed.
    // Symlinks are NOT resolved, so `link.txt -> file.txt` and `file.txt`
    // remain separate entries (they are distinct filesystem entries).
    {
        let mut seen = std::collections::HashSet::new();
        files.retain(|p| {
            let key = awara::normalize_path_dedup(p);
            seen.insert(key)
        });
    }

    // Apply sort mode using shared sort_paths (SSoT with TUI and GUI)
    // numbering_source maps to the same underlying sort comparators.
    let sort_by: Option<ScanSortBy> = args
        .sort
        .as_deref()
        .and_then(|s| match s {
            "name" => Some(ScanSortBy::Name),
            "date" => Some(ScanSortBy::Date),
            "size" => Some(ScanSortBy::Size),
            "name_desc" => Some(ScanSortBy::NameDesc),
            "date_desc" => Some(ScanSortBy::DateDesc),
            "size_desc" => Some(ScanSortBy::SizeDesc),
            "ext" | "extension" => Some(ScanSortBy::Extension),
            "ext_desc" | "extension_desc" => Some(ScanSortBy::ExtensionDesc),
            _ => None,
        })
        .or_else(|| {
            config.numbering.numbering_source.map(|s| match s {
                NumberingSource::Name => ScanSortBy::Name,
                NumberingSource::Date => ScanSortBy::Date,
                NumberingSource::Size => ScanSortBy::Size,
            })
        });
    if let Some(sort_by) = sort_by {
        let sort_opts = ScanOptions {
            sort_by: Some(sort_by),
            ..Default::default()
        };
        sort_paths(&mut files, &sort_opts);
    } else {
        // Default: natural sort by filename
        let sort_opts = ScanOptions {
            sort_by: Some(ScanSortBy::Name),
            ..Default::default()
        };
        sort_paths(&mut files, &sort_opts);
    }

    // keep-structure base: the common ancestor of the input paths, so the
    // relative subfolder layout is recreated under --output-dir (falls back
    // to flat when inputs share no common ancestor).
    let keep_structure = args.keep_structure;
    let keep_base = if keep_structure {
        awara::common_base_dir(&files)
    } else {
        None
    };

    // Resolve a relative --output-dir against the launch directory (shared
    // SSoT rule with GUI/TUI via resolve_output_dir).
    let output_dir = args.output_dir.as_deref().map(|d| {
        awara::resolve_output_dir(
            d,
            &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        )
    });

    let files_str: Vec<String> = files
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Build the plan once: collision detection and execution share it, so the
    // reported conflicts and the applied names cannot diverge.
    let plan_opts = awara::PlanOptions {
        dirname_level: args.dirname_level,
        output_dir: output_dir.as_deref(),
        keep_structure,
        base_dir: keep_base.as_deref(),
        parallel: true,
        // Copy mode is part of the plan (it decides whether a source is
        // vacated, and therefore collision classification).
        copy_mode: output_dir.is_some() && config.copy_to.copy_mode,
    };
    let plan = awara::plan_renames(&files_str, &config, &plan_opts);

    // Detected once: the prompt counts, the folder warning, and execution
    // all read the same collision set.
    let collisions = plan.collisions();
    let folder_count = collisions.iter().filter(|c| c.folder_involved()).count();

    // Determine collision strategy.
    // --overwrite / --skip pre-commit to a policy applied to all collisions;
    // otherwise prompt the user when collisions exist. When stdin isn't a TTY
    // (scripts/pipes) there's nobody to ask — default to the safe Skip.
    let collision_strategy = if args.overwrite {
        CollisionStrategy::Overwrite
    } else if args.skip || args.dry_run {
        CollisionStrategy::Skip
    } else {
        let collision_count = collisions.len();
        if collision_count == 0 {
            CollisionStrategy::Skip
        } else if io::stdin().is_terminal() {
            eprintln!(
                "{} target file(s) already exist and would be overwritten.",
                collision_count
            );
            print!("Overwrite all? [y/N] ");
            io::stdout().flush().ok();
            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            if input.trim().to_lowercase() == "y" {
                CollisionStrategy::Overwrite
            } else {
                eprintln!(
                    "Skipping collisions. Use --overwrite to overwrite all, or --skip to skip without prompting."
                );
                CollisionStrategy::Skip
            }
        } else {
            eprintln!(
                "{} target file(s) already exist; skipping. Use --overwrite to overwrite them.",
                collision_count
            );
            CollisionStrategy::Skip
        }
    };

    // A folder on either side is never overwritten or merged, regardless of the
    // strategy, so say so up front instead of letting the run report silent skips.
    if folder_count > 0 {
        eprintln!(
            "{} collision(s) involve a folder; folders cannot be overwritten or merged and will be skipped.",
            folder_count
        );
    }

    // ── Signal handling for graceful cancellation ──
    // Direct signal handler: Ctrl+C during rename sets the cancellation token.
    // The rename loop checks this token between each file operation.
    let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let cancel_signal = std::sync::Arc::clone(&cancel_token);
        let _ = ctrlc::set_handler(move || {
            cancel_signal.store(true, std::sync::atomic::Ordering::SeqCst);
        });
    }

    // Use shared pipeline (SSoT with TUI). Planning (output dir, dirname level,
    // parallelism) lives on the plan; options carries execution behavior only.
    let mut options = RenameOptions {
        dry_run: args.dry_run,
        stop_on_error: config.stop_on_error,
        preserve_timestamps: args.preserve_timestamps,
        set_attributes: args.set_attributes.as_deref(),
        undo_file: args.undo_file.as_deref(),
        cancel_token: Some(cancel_token.clone()),
        on_event: Some(Box::new(move |event| match event {
            RenameEvent::Progress { original, new } => {
                if args.dry_run || args.verbose {
                    println!("'{}' -> '{}'", original, new);
                }
            }
            RenameEvent::Collision { original, new } => {
                eprintln!(
                    "Collision detected: '{}' -> '{}' (target exists)",
                    original, new
                );
            }
            RenameEvent::Error {
                original,
                new,
                message,
            } => {
                eprintln!("Error renaming '{}' -> '{}': {}", original, new, message);
            }
            RenameEvent::Skipped { original, reason } => {
                println!("Skipping '{}': {}", original, reason);
            }
            RenameEvent::RollbackProgress { original, new } => {
                println!("Undid: '{}' -> '{}'", new, original);
            }
            RenameEvent::Status { message } => {
                eprintln!("{}", message);
            }
            RenameEvent::Cancelled { completed } => {
                eprintln!(
                    "\nCancelled after {} file(s) renamed. Partial undo file written.",
                    completed
                );
            }
            RenameEvent::Done {
                success,
                errors,
                skipped,
            } => {
                if args.verbose {
                    println!(
                        "Done: {} renamed, {} errors, {} skipped",
                        success, errors, skipped
                    );
                }
            }
        })),
    };

    let resolution = awara::Resolution::default_for(collision_strategy);
    let result = awara::execute_plan(plan, &config, &mut options, &resolution);

    // If cancelled, exit with conventional SIGINT code (130 = 128 + SIGINT)
    if cancel_token.load(std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(130);
    }

    // Exit with error code if any failures occurred
    if !result.failed_ops.is_empty() {
        std::process::exit(1);
    }
}

fn swap_files(args: &Args) {
    if args.files.len() < 2 {
        eprintln!("Error: --swap requires at least 2 files");
        std::process::exit(1);
    }
    if !args.files.len().is_multiple_of(2) {
        eprintln!("Error: --swap requires an even number of files (pairs)");
        std::process::exit(1);
    }

    for chunk in args.files.chunks(2) {
        let path1 = Path::new(&chunk[0]);
        let path2 = Path::new(&chunk[1]);

        if !path1.exists() {
            eprintln!("Error: '{}' does not exist", chunk[0]);
            continue;
        }
        if !path2.exists() {
            eprintln!("Error: '{}' does not exist", chunk[1]);
            continue;
        }

        if args.dry_run || args.verbose {
            println!("Swap: '{}' <-> '{}'", chunk[0], chunk[1]);
        }

        if args.dry_run {
            continue;
        }

        // Use temp file to swap
        let tmp = path1.with_extension(format!("tmp_{}", uuid::Uuid::new_v4()));
        match fs::rename(path1, &tmp) {
            Ok(()) => {
                match fs::rename(path2, path1) {
                    Ok(()) => {
                        match fs::rename(&tmp, path2) {
                            Ok(()) => {
                                println!("Swapped: '{}' <-> '{}'", chunk[0], chunk[1]);
                            }
                            Err(e) => {
                                eprintln!("Failed to rename temp back: {}", e);
                                let _ = fs::rename(&tmp, path1); // Restore
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to rename second file: {}", e);
                        let _ = fs::rename(&tmp, path1); // Restore
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to rename first file: {}", e);
            }
        }
    }
}
