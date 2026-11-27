//! One worker owns network and disk work. A channel serializes restart requests with checks.
//! On macOS and Windows it downloads a newer release as soon as it finds it, then asks the user
//! to restart, checking once more then so the restart lands on the newest release; on Linux it
//! only checks: the user installs the release with the command it offers.

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime};

use rune::AppHandle;

use super::install::{Install, Staged};
use super::{Release, Upgrade, linux, release};
use crate::state::State;

const HOUR: Duration = Duration::from_secs(60 * 60);
/// The longest the worker sleeps at once, so a wall clock that jumped (sleep, a time change)
/// is noticed soon.
const GLANCE: Duration = Duration::from_secs(5 * 60);

pub fn start(app: AppHandle<State>, cache: PathBuf) -> Option<Sender<Release>> {
    let (sender, receiver) = mpsc::channel::<Release>();
    let spawned = std::thread::Builder::new().name("updates".into()).spawn(move || {
        let result: Result<(), String> = (|| {
            let (target, install) = if cfg!(target_os = "linux") {
                (release::Target::Linux(linux::Method::detect(&linux::Facts::gather())), None)
            } else {
                let Some(install) = Install::current()? else { return Ok(()) };
                (release::Target::Archive(install.platform()), Some(install))
            };
            let error_file = cache.join("error.txt");
            if let Some(install) = &install {
                install.clear_stale();
                std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
                if let Ok(error) = std::fs::read_to_string(&error_file) {
                    let _ = std::fs::remove_file(&error_file);
                    eprintln!("better-matrix: the previous update did not complete: {}", error.trim());
                    app.update(|s, _| s.update_interrupted());
                }
            }
            let client = release::client()?;
            let mut schedule = Schedule::new(SystemTime::now());
            let mut staged = None::<Staged>;
            let refresh = |staged: &mut Option<Staged>| match release::check(&client, &target) {
                Ok(Some(release)) => match &install {
                    // Only a newer release than the one waiting replaces it, once staged.
                    Some(install) if staged.as_ref().is_none_or(|ready| newer(&release.version, &ready.version)) => {
                        match install.stage(&client, &release) {
                            Ok(ready) => {
                                *staged = Some(ready);
                                app.update(move |s, _| s.update_ready(release));
                            }
                            Err(error) => {
                                eprintln!("better-matrix: update download failed: {error}");
                                // Retried at the next check; until then, its page has it.
                                if staged.is_none() {
                                    let page = Upgrade::Page(linux::release_page(&release.version));
                                    app.update(move |s, _| s.update_found(Release { upgrade: page, ..release }));
                                }
                            }
                        }
                    }
                    Some(_) => {}
                    None => app.update(move |s, _| s.update_found(release)),
                },
                Ok(None) => {}
                // A laptop can be offline: retry on the next hourly check, without a notice.
                Err(error) => eprintln!("better-matrix: update check failed: {error}"),
            };
            loop {
                if schedule.due(SystemTime::now()) {
                    refresh(&mut staged);
                }
                match receiver.recv_timeout(schedule.wait(SystemTime::now())) {
                    // The user restarts into the newest release: one published since the last
                    // check replaces what is staged first, so a copy several releases behind
                    // never restarts into one that is already outdated.
                    Ok(_) => {
                        let Some(install) = &install else { continue };
                        refresh(&mut staged);
                        let Some(ready) = &staged else {
                            app.update(|s, _| s.update_failed("the update is not downloaded".into()));
                            continue;
                        };
                        match install.hand_over(ready, &error_file) {
                            Ok(()) => {
                                if let Some(ready) = staged.take() {
                                    ready.keep();
                                }
                                app.update(|_, tx| tx.quit());
                                return Ok(());
                            }
                            // What was staged may be gone (an antivirus took it): stage afresh
                            // now, and the next restart uses that. Meanwhile its page has it.
                            Err(error) => {
                                let version = ready.version.clone();
                                staged = None;
                                schedule = Schedule::new(SystemTime::now());
                                let page = Upgrade::Page(linux::release_page(&version));
                                app.update(move |s, _| {
                                    s.update_failed(error);
                                    s.update_found(Release { version, upgrade: page });
                                });
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                }
            }
        })();
        if let Err(error) = result {
            eprintln!("better-matrix: updater could not start: {error}");
        }
    });
    match spawned {
        Ok(_) => Some(sender),
        Err(error) => {
            eprintln!("better-matrix: could not start updater: {error}");
            None
        }
    }
}

/// Whether version `a` comes after `b`.
fn newer(a: &str, b: &str) -> bool {
    match (semver::Version::parse(a), semver::Version::parse(b)) {
        (Ok(a), Ok(b)) => a.cmp_precedence(&b).is_gt(),
        _ => a != b,
    }
}

/// The hourly check, by the wall clock: a laptop asleep for a day checks soon after it wakes, and
/// no hour of sleep is replayed. The monotonic clock some systems stop during sleep only bounds
/// each wait.
#[derive(Debug)]
struct Schedule {
    next: SystemTime,
}

impl Schedule {
    fn new(now: SystemTime) -> Self {
        Self { next: now }
    }

    fn due(&mut self, now: SystemTime) -> bool {
        // A clock set back by more than an hour would otherwise hold checks off until it caught up.
        if now < self.next && self.next.duration_since(now).is_ok_and(|left| left <= HOUR) {
            return false;
        }
        self.next = now + HOUR;
        true
    }

    /// How long to sleep before looking again: until the check is due, at most [`GLANCE`].
    fn wait(&self, now: SystemTime) -> Duration {
        self.next.duration_since(now).unwrap_or_default().min(GLANCE)
    }
}

#[cfg(test)]
mod tests;
