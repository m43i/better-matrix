use std::fs;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::args::Cli;
use crate::skill::{self, REFERENCES};
use crate::terminal::Terminal;

#[test]
fn every_command_should_have_a_reference_example() {
    fn check(command: &clap::Command, group: &str) {
        let path = format!("{group} {}", command.get_name());
        if command.get_subcommands().next().is_none() {
            let heading = format!("## `{path}`");
            assert!(REFERENCES.iter().any(|(_, text)| text.lines().any(|line| line == heading)), "{path}");
        }
        for subcommand in command.get_subcommands() {
            check(subcommand, &path);
        }
    }

    for command in Cli::command().get_subcommands() {
        check(command, "bm");
    }
}

#[test]
fn every_command_example_should_parse() {
    for (name, text) in REFERENCES {
        let mut example = false;
        for line in text.lines() {
            if line == "```bash" {
                example = true;
            } else if line == "```" {
                example = false;
            } else if example {
                let args = shlex::split(line).expect("valid shell quoting");
                if let Err(error) = Cli::try_parse_from(args) {
                    assert!(
                        matches!(error.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion),
                        "{name}: {line}\n{error}"
                    );
                }
            }
        }
    }
}

#[test]
fn an_install_should_write_all_the_references_to_both_destinations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roots = [dir.path().join(".agents/skills/better-matrix"), dir.path().join(".claude/skills/better-matrix")];
    let terminal = Terminal { json: true, no_input: true, yes: false };
    assert_eq!(skill::write_bundle(terminal, &roots).expect("install"), 2 * (REFERENCES.len() + 1));
    for root in &roots {
        let entry = fs::read_to_string(root.join("SKILL.md")).expect("entry");
        for (path, text) in REFERENCES {
            assert!(entry.contains(path));
            assert_eq!(fs::read_to_string(root.join(path)).expect("reference"), *text);
        }
    }
    assert_eq!(skill::write_bundle(terminal, &roots).expect("reinstall"), 0);
}

#[test]
fn a_noninteractive_install_should_preserve_an_edited_skill() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roots = [dir.path().join(".agents/skills/better-matrix")];
    let terminal = Terminal { json: true, no_input: true, yes: false };
    skill::write_bundle(terminal, &roots).expect("install");
    let entry = roots[0].join("SKILL.md");
    fs::write(&entry, "local edits").expect("change");
    assert!(skill::write_bundle(terminal, &roots).is_err());
    assert_eq!(fs::read_to_string(entry).expect("entry"), "local edits");
}

#[test]
fn a_second_process_should_get_an_error_opening_the_same_store() {
    let dir = tempfile::tempdir().expect("temp dir");
    let _lock = crate::terminal::lock(dir.path()).expect("first process");
    assert!(crate::terminal::lock(dir.path()).is_err());
}

#[test]
fn every_cli_catalog_should_use_the_same_keys() {
    let catalogs = [include_str!("../i18n/en.json"), include_str!("../i18n/de.json"), include_str!("../i18n/fr.json")];
    let keys = catalogs.map(|catalog| {
        serde_json::from_str::<std::collections::BTreeMap<String, String>>(catalog)
            .expect("valid catalog")
            .into_keys()
            .collect::<Vec<_>>()
    });
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[0], keys[2]);
}

#[cfg(unix)]
#[test]
fn a_local_install_should_reject_a_linked_skill_directory() {
    let local = tempfile::tempdir().expect("local directory");
    let global = tempfile::tempdir().expect("user directory");
    std::os::unix::fs::symlink(global.path(), local.path().join(".agents")).expect("link to global");
    assert!(skill::check_local_parents(local.path(), &local.path().join(".agents/skills/better-matrix")).is_err());
}

#[test]
fn a_key_export_should_refuse_an_existing_destination() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("keys.txt");
    fs::write(&dest, "existing export").expect("existing file");
    assert!(crate::terminal::temporary_output(&dest).is_err());
}
