use super::*;
use awara::RenameConfig;

fn parse_ok(input: &str, config: &mut RenameConfig) -> Result<Vec<ParseAction>, String> {
    parse_command(input, config)
}

#[test]
fn test_parse_prefix() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--add-prefix test_", &mut config).is_ok());
    assert_eq!(config.add.add_prefix, Some("test_".into()));
}

#[test]
fn test_parse_suffix() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--add-suffix _bak", &mut config).is_ok());
    assert_eq!(config.add.add_suffix, Some("_bak".into()));
}

#[test]
fn test_parse_replace() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--replace foo --with bar", &mut config).is_ok());
    assert_eq!(config.replace.replace, Some("foo".into()));
    assert_eq!(config.replace.with, Some("bar".into()));
}

#[test]
fn test_parse_regex() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--regex-match [0-9]+ --regex-replace NUM", &mut config).is_ok());
    assert_eq!(config.regex.regex_match, Some("[0-9]+".into()));
}

#[test]
fn test_parse_case() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--case-name lower", &mut config).is_ok());
    assert_eq!(config.case.case_name, Some(awara::CaseMode::Lower));
}

#[test]
fn test_parse_remove_positional() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--remove-first 3 --remove-last 2", &mut config).is_ok());
    assert_eq!(config.remove.remove_first, Some(3));
    assert_eq!(config.remove.remove_last, Some(2));
}

#[test]
fn test_parse_numbering() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--numbering-mode prefix --numbering-start 1", &mut config).is_ok());
    assert_eq!(
        config.numbering.numbering_mode,
        Some(awara::NumberingMode::Prefix)
    );
}

#[test]
fn test_parse_extension() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--extension-mode lower --extension .bak", &mut config).is_ok());
    assert_eq!(
        config.extension.extension_mode,
        Some(awara::ExtensionMode::Lower)
    );
}

#[test]
fn test_parse_dirname() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--add-dirname --dirname-sep _", &mut config).is_ok());
    assert!(config.append_folder.add_dirname);
    assert_eq!(config.append_folder.add_dirname_sep, Some("_".into()));
}

#[test]
fn test_parse_move_copy() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--move-part 0:3:5", &mut config).is_ok());
    assert!(config.move_copy.move_part.is_some());
}

#[test]
fn test_parse_date() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--add-date %Y-%m-%d", &mut config).is_ok());
    assert_eq!(config.auto_date.add_date, Some("%Y-%m-%d".into()));
}

#[test]
fn test_parse_multiple_flags_replaces_previous() {
    // Each command fully replaces config, so second call clears first
    let mut config = RenameConfig::default();
    assert!(parse_ok("--add-prefix A_", &mut config).is_ok());
    assert_eq!(config.add.add_prefix, Some("A_".into()));

    // Second command replaces entirely
    assert!(parse_ok("--add-suffix _B", &mut config).is_ok());
    assert_eq!(config.add.add_prefix, None); // cleared by replacement
    assert_eq!(config.add.add_suffix, Some("_B".into()));
}

#[test]
fn test_parse_unknown_flag_returns_error() {
    let mut config = RenameConfig::default();
    let result = parse_command("--foobar", &mut config);
    assert!(result.is_err());
}

#[test]
fn test_parse_non_flag_returns_error() {
    let mut config = RenameConfig::default();
    let result = parse_command("unknown-command", &mut config);
    assert!(result.is_err());
}

#[test]
fn test_parse_empty_input() {
    let mut config = RenameConfig::default();
    let result = parse_command("", &mut config);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_parse_reset() {
    let mut config = RenameConfig::default();
    config.add.add_prefix = Some("test_".into());
    assert!(parse_ok("reset", &mut config).is_ok());
    assert_eq!(config.add.add_prefix, None);
}

#[test]
fn test_parse_quotes() {
    let mut config = RenameConfig::default();
    assert!(parse_ok("--replace \"foo bar\" --with baz", &mut config).is_ok());
    assert_eq!(config.replace.replace, Some("foo bar".into()));
    assert_eq!(config.replace.with, Some("baz".into()));
}

#[test]
fn test_parse_sort() {
    let mut config = RenameConfig::default();
    let result = parse_command("sort name", &mut config);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        vec![ParseAction::UpdateSort(awara::ScanSortBy::Name)]
    );
}

/// The command registry (SSoT) must be internally consistent: parser
/// commands carry a parse handler, events commands don't, every name and
/// alias resolves through `find`, names/aliases are unique, and
/// `available_commands` lists them all.
#[test]
fn test_command_registry_consistency() {
    for meta in CONSOLE_COMMANDS {
        match meta.handled_by {
            HandledBy::Parser => assert!(
                meta.parse.is_some(),
                "parser command '{}' needs a parse handler",
                meta.name
            ),
            HandledBy::Events => assert!(
                meta.parse.is_none(),
                "events command '{}' should not have a parse handler",
                meta.name
            ),
        }

        assert_eq!(
            find(meta.name).map(|m| m.name),
            Some(meta.name),
            "find('{}') must resolve to itself",
            meta.name
        );
        for alias in meta.aliases {
            assert_eq!(
                find(alias).map(|m| m.name),
                Some(meta.name),
                "find('{alias}') must resolve to '{}'",
                meta.name
            );
        }
        assert!(
            available_commands().contains(meta.name),
            "'{}' missing from available_commands()",
            meta.name
        );
    }

    // Names and aliases must be unique across the whole table.
    let mut tokens: Vec<&str> = CONSOLE_COMMANDS
        .iter()
        .flat_map(|m| std::iter::once(m.name).chain(m.aliases.iter().copied()))
        .collect();
    tokens.sort();
    for pair in tokens.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "duplicate command name/alias '{}'",
            pair[0]
        );
    }
}

/// Every declared console command (name and aliases) must be recognized by
/// the dispatcher — i.e. never rejected as an unknown command. This is the
/// completeness guard: a command added to the parser without a registry
/// entry would be rejected here.
#[test]
fn test_every_declared_command_is_recognized() {
    for meta in CONSOLE_COMMANDS {
        for name in std::iter::once(meta.name).chain(meta.aliases.iter().copied()) {
            let mut config = RenameConfig::default();
            match parse_command(name, &mut config) {
                Ok(_) => {}
                Err(e) => assert!(
                    !e.contains("unknown command"),
                    "'{}' must be a recognized command, got: {}",
                    name,
                    e
                ),
            }
        }
    }
}

/// The TUI reference must document every console command declared in the
/// registry, so a new command can't ship without docs.
#[test]
fn test_console_commands_are_documented() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/tui.md"))
        .expect("read docs/tui.md");

    let missing: Vec<&str> = CONSOLE_COMMANDS
        .iter()
        .map(|m| m.name)
        .filter(|name| !doc.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "console commands missing from docs/tui.md: {missing:?}"
    );
}
