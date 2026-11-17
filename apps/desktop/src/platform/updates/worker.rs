//! One worker owns network and disk work. A channel serializes install requests with checks.

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use rune::AppHandle;

use super::{Release, install::Install, release};
use crate::state::State;

const HOUR: Duration = Duration::from_secs(60 * 60);

pub fn start(app: AppHandle<State>, cache: PathBuf) -> Option<Sender<Release>> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("updates".into()).spawn(move || {
        let result: Result<(), String> = (|| {
            let Some(install) = Install::current()? else { return Ok(()) };
            std::fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
            let error_file = cache.join("error.txt");
            if let Ok(error) = std::fs::read_to_string(&error_file) {
                let _ = std::fs::remove_file(&error_file);
                eprintln!("better-matrix: the previous update did not complete: {}", error.trim());
                app.update(|s, _| s.update_interrupted());
            }
            let client = release::client()?;
            let mut schedule = Schedule::new(Instant::now());
            loop {
                let now = Instant::now();
                if schedule.due(now) {
                    match release::check(&client, install.platform()) {
                        Ok(Some(release)) => app.update(move |s, _| s.update_found(release)),
                        Ok(None) => {}
                        // A laptop can be offline: retry on the next hourly check, without a notice.
                        Err(error) => eprintln!("better-matrix: update check failed: {error}"),
                    }
                }
                match receiver.recv_timeout(schedule.wait(Instant::now())) {
                    Ok(release) => match install.prepare(&client, &release, &error_file) {
                        Ok(()) => {
                            app.update(|_, tx| tx.quit());
                            return Ok(());
                        }
                        Err(error) => app.update(move |s, _| s.update_failed(error)),
                    },
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
