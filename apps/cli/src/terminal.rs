use crate::text::t;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{CliError, Result};

pub(crate) fn home() -> Result<PathBuf> {
    env::home_dir().ok_or_else(|| CliError::Input(t("could not find your home directory").into()))
}

pub(crate) fn data_directory(supplied: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = supplied {
        return Ok(path);
    }
    if let Some(path) = env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|path| path.is_absolute()) {
        return Ok(path.join("better-matrix-cli"));
    }
    Ok(home()?.join(".local/share/better-matrix-cli"))
}

pub(crate) fn lock(path: &Path) -> Result<File> {
    let mut directory = fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(path)?;
    let file = File::options().read(true).write(true).create(true).truncate(false).open(path.join("store.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(CliError::Input(tf!(
            "another bm process is using {path}; pass --data-dir to use a separate store",
            path = path.display()
        ))),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

pub(crate) fn read_default(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path.join("default-account")) {
        Ok(text) => Ok(Some(text.trim().to_owned()).filter(|text| !text.is_empty())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn set_default(path: &Path, id: &str) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path)?;
    writeln!(file, "{id}")?;
    file.persist(path.join("default-account")).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn temporary_output(path: &Path) -> Result<tempfile::NamedTempFile> {
    if path.symlink_metadata().is_ok() {
        return Err(CliError::Input(tf!("file already exists: {path}", path = path.display())));
    }
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    Ok(tempfile::NamedTempFile::new_in(parent)?)
}

pub(crate) fn clear_default(path: &Path) -> Result<()> {
    match fs::remove_file(path.join("default-account")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Terminal {
    pub json: bool,
    pub no_input: bool,
    pub yes: bool,
}

impl Terminal {
    pub fn interactive(self) -> bool {
        !self.no_input && io::stdin().is_terminal()
    }

    pub fn ask(self, prompt: &str) -> Result<String> {
        if !self.interactive() {
            return Err(CliError::Input(tf!("input is required: {prompt}", prompt = prompt)));
        }
        let mut err = io::stderr().lock();
        write!(err, "{prompt} ")?;
        err.flush()?;
        drop(err);
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Err(CliError::Input(t("input ended").into()));
        }
        Ok(line.trim().to_owned())
    }

    pub fn secret(self, prompt: &str, stdin: bool) -> Result<String> {
        let value = if stdin {
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line)?;
            line.trim_end_matches(['\r', '\n']).to_owned()
        } else if self.interactive() {
            rpassword::prompt_password(prompt)?
        } else {
            return Err(CliError::Input(t("provide the secret using its stdin option").into()));
        };
        if value.is_empty() {
            return Err(CliError::Input(t("the secret is empty").into()));
        }
        Ok(value)
    }

    pub fn confirm(self, prompt: &str) -> Result<()> {
        if self.yes {
            return Ok(());
        }
        if !self.interactive() {
            return Err(CliError::Input(tf!("{prompt} Pass --yes to confirm.", prompt = prompt)));
        }
        if !matches!(self.ask(&format!("{prompt} [y/N]"))?.to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err(CliError::Input(t("cancelled").into()));
        }
        Ok(())
    }

    pub fn output(self, value: &Value) -> Result<()> {
        let mut out = io::stdout().lock();
        if self.json {
            serde_json::to_writer(&mut out, value)?;
            writeln!(out)?;
            return Ok(());
        }
        if let Some(rows) = value.as_array() {
            for row in rows {
                writeln!(out, "{}", safe(&human_row(row)?))?;
            }
        } else if let Some(rows) = value.get("messages").and_then(Value::as_array) {
            for row in rows {
                writeln!(out, "{}", safe(&human_row(row)?))?;
            }
            if let Some(cursor) = value["next_cursor"].as_str() {
                writeln!(out, "{}  {}", t("Next cursor"), safe(cursor))?;
            }
            if value["scan_limit_reached"] == true {
                writeln!(out, "{}", t("Some rooms reached the history scan limit."))?;
            }
            if let Some(count) = value["undecryptable"].as_u64().filter(|&count| count > 0) {
                writeln!(out, "{}  {count}", t("Messages missing keys"))?;
            }
        } else if value.get("sender_id").is_some() && value.get("event_id").is_some() {
            writeln!(out, "{}", safe(&human_row(value)?))?;
        } else if value["ok"] == true && value.as_object().is_some_and(|obj| obj.len() == 1) {
            writeln!(out, "{}", t("OK"))?;
        } else {
            writeln!(out, "{}", serde_json::to_string_pretty(value)?)?;
        }
        Ok(())
    }
}

pub(crate) fn safe(text: &str) -> String {
    text.chars().filter(|&character| matches!(character, '\n' | '\t')
            || (!character.is_control()
                && !matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))).collect()
}

fn human_row(value: &Value) -> Result<String> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_owned());
    }
    let event = value["event_id"].as_str().or_else(|| value["root"].as_str());
    if let Some(id) = event {
        let sender = value["sender_id"].as_str().unwrap_or_default();
        let body = value["body"]
            .as_str()
            .or_else(|| value["attachment"]["name"].as_str())
            .unwrap_or_else(|| if value["undecryptable"] == true { t("[missing room key]") } else { "" });
        return Ok(format!("{id}\t{sender}\t{}", body.replace(['\n', '\t'], " ")));
    }
    let id = value["room_id"].as_str().or_else(|| value["user_id"].as_str()).or_else(|| value["id"].as_str());
    if let Some(id) = id {
        let label = value["name"].as_str().or_else(|| value["display_name"].as_str()).unwrap_or_default();
        return Ok(format!("{id}\t{}", label.replace(['\n', '\t'], " ")));
    }
    Ok(serde_json::to_string(value)?)
}
