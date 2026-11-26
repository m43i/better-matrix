//! One worker owns network and disk work. A channel serializes restart requests with checks.
//! On macOS and Windows it downloads a newer release as soon as it finds it, then asks the user
//! to restart; on Linux it only checks: the user installs the release with the command it offers.

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use rune::AppHandle;

use super::install::{Install, Staged};
use super::{Release, Upgrade, linux, release};
use crate::state::State;

const HOUR: Duration = Duration::from_secs(60 * 60);

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
            let mut schedule = Schedule::new(Instant::now());
            let mut staged = None::<Staged>;
            loop {
                let now = Instant::now();
                if schedule.due(now) {
                    match release::check(&client, &target) {
                        Ok(Some(release)) => match &install {
                            // Only a newer release than the one waiting replaces it, once staged.
                            Some(install)
                                if staged.as_ref().is_none_or(|ready| newer(&release.version, &ready.version)) =>
                            {
                                match install.stage(&client, &release) {
                                    Ok(ready) => {
                                        staged = Some(ready);
                                        app.update(move |s, _| s.update_ready(release));
                                    }
                                    Err(error) => {
                                        eprintln!("better-matrix: update download failed: {error}");
                                        // Retried at the next check; until then, its page has it.
                                        if staged.is_none() {
                                            let page = Upgrade::Page(linux::release_page(&release.version));
                                            app.update(move |s, _| {
                                                s.update_found(Release { upgrade: page, ..release })
                                            });
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
                    }
                }
                match receiver.recv_timeout(schedule.wait(Instant::now())) {
                    // The user restarts into what is staged: a newer release staged since the
                    // rail offered one is the better one to restart into.
                    Ok(_) => {
                        let Some(install) = &install else { continue };
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
                                schedule = Schedule::new(Instant::now());
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

#[derive(Debug)]
struct Schedule {
    next: Instant,
}

impl Schedule {
    fn new(now: Instant) -> Self {
        Self { next: now }
    }

    fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        // After sleep, check once; don't replay missed hours.
        self.next = now + HOUR;
        true
    }

    fn wait(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }
}

#[cfg(test)]
mod tests;
