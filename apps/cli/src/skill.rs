use crate::text::t;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::args::{Scope, Skill};
use crate::terminal::{Terminal, home};
use crate::{CliError, Result};

const ENTRY: &str = include_str!("../skill/SKILL.md");

macro_rules! references {
    ($($name:literal),+ $(,)?) => {
        &[$((concat!("references/", $name, ".md"), include_str!(concat!("../skill/references/", $name, ".md")))),+]
    };
}

pub(crate) const REFERENCES: &[(&str, &str)] = references!(
    "auth",
    "rooms",
    "spaces",
    "dms",
    "invites",
    "messages",
    "threads",
    "pins",
    "saved",
    "files",
    "mentions",
    "notifications",
    "people",
    "profile",
    "report",
    "cli",
);

pub(crate) fn install(terminal: Terminal, command: Option<Skill>) -> Result<Value> {
    let scope = match command {
        Some(Skill::Install { scope: Some(scope) }) => scope,
        _ => ask_scope(terminal)?,
    };
    let base = match scope {
        Scope::Local => env::current_dir()?,
        Scope::Global => home()?,
    };
    let claude_config = env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
    let default_config = home().ok().map(|dir| dir.join(".claude"));
    let claude = base.join(".claude").is_dir()
        || claude_config.as_ref().is_some_and(|dir| dir.is_dir())
        || default_config.as_ref().is_some_and(|dir| dir.is_dir())
        || env::var_os("PATH").is_some_and(|paths| {
            env::split_paths(&paths).any(|path| path.join(format!("claude{}", env::consts::EXE_SUFFIX)).is_file())
        });
    let mut roots = vec![base.join(".agents/skills/better-matrix")];
    if claude {
        let config = match scope {
            Scope::Local => base.join(".claude"),
            Scope::Global => claude_config.unwrap_or_else(|| base.join(".claude")),
        };
        roots.push(config.join("skills/better-matrix"));
    }
    if matches!(scope, Scope::Local) {
        for root in &roots {
            check_local_parents(&base, root)?;
        }
    }
    let changed = write_bundle(terminal, &roots)?;
    Ok(
        json!({ "scope": match scope { Scope::Local => "local", Scope::Global => "global" }, "paths": roots, "changed": changed }),
    )
}

fn ask_scope(terminal: Terminal) -> Result<Scope> {
    if !terminal.interactive() {
        return Err(CliError::Input(t("choose --scope local or --scope global").into()));
    }
    loop {
        match terminal
            .ask(t("Install in the current directory or globally for your user? [local/global]"))?
            .to_ascii_lowercase()
            .as_str()
        {
            "local" | "l" => return Ok(Scope::Local),
            "global" | "g" => return Ok(Scope::Global),
            _ => eprintln!("{}", t("Choose local or global.")),
        }
    }
}

#[derive(Debug)]
struct Pending {
    destination: PathBuf,
    contents: &'static str,
    previous: Option<Vec<u8>>,
}

fn plan(roots: &[PathBuf]) -> Result<Vec<Pending>> {
    let mut pending = Vec::new();
    for root in roots {
        check_directory(root)?;
        check_directory(&root.join("references"))?;
        for (relative, contents) in std::iter::once(("SKILL.md", ENTRY)).chain(REFERENCES.iter().copied()) {
            let path = root.join(relative);
            let previous = match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => Some(fs::read(&path)?),
                Ok(_) => return Err(CliError::Input(tf!("expected a regular file at {path}", path = path.display()))),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if previous.as_deref() != Some(contents.as_bytes()) {
                pending.push(Pending { destination: path, contents, previous });
            }
        }
    }
    Ok(pending)
}

pub(crate) fn check_local_parents(base: &Path, root: &Path) -> Result<()> {
    // A local install must not follow a linked .agents or .claude into a global installation.
    for directory in root.ancestors().take_while(|ancestor| *ancestor != base) {
        check_directory(directory)?;
    }
    Ok(())
}

fn check_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(CliError::Input(tf!("expected a directory at {path}", path = path.display()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn write_bundle(terminal: Terminal, roots: &[PathBuf]) -> Result<usize> {
    let pending = plan(roots)?;
    let replacements: Vec<_> = pending.iter().filter(|file| file.previous.is_some()).collect();
    if !replacements.is_empty() {
        for file in replacements {
            eprintln!("{}", crate::terminal::safe(&file.destination.display().to_string()));
        }
        terminal.confirm(t("Replace the listed skill files?"))?;
    }
    for entry in &pending {
        let parent = entry.destination.parent().ok_or(CliError::NoResult)?;
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(entry.contents.as_bytes())?;
        if let Some(previous) = &entry.previous {
            if fs::read(&entry.destination)? != *previous {
                return Err(CliError::Input(tf!(
                    "{path} changed during installation",
                    path = entry.destination.display()
                )));
            }
            file.persist(&entry.destination).map_err(|error| error.error)?;
        } else {
            file.persist_noclobber(&entry.destination).map_err(|error| error.error)?;
        }
    }
    Ok(pending.len())
}
