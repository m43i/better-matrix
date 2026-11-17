//! Whether the app is in front, which decides how syncs share an account's store with pushes
//! handled without the engine (ADR 0010).
//!
//! A sync on a shared store holds the crypto store's cross-process lock, whose lease the SDK
//! writes to the store every 50 ms while held: through a long poll, 20 writes a second. In front
//! no push is handled outside the engine (iOS's extension stays out, Android's push task asks the
//! engine), so once a push started before the app came to the front can no longer be running,
//! syncs take the lock only for a moment first (which waits for a push and reloads what it wrote)
//! and run without it. Leaving the front ends those syncs before [`Front::set`] finishes.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedRwLockReadGuard, RwLock, watch};
use tokio::time::Instant;

/// How long after coming to the front a push handled without the engine may still run: its
/// own time limit.
const PUSH_MAY_RUN: Duration = crate::push::WITHOUT_ENGINE;

/// The app in front or not, shared by the engine and every sync loop.
#[derive(Clone, Debug)]
pub(crate) struct Front {
    /// Since when the app is in front; `None` while it isn't. The engine starts out of front.
    since: watch::Sender<Option<Instant>>,
    /// Held for reading by each sync running without the lock; leaving the front waits to
    /// hold it for writing.
    unlocked: Arc<RwLock<()>>,
}

impl Default for Front {
    fn default() -> Self {
        Self { since: watch::Sender::new(None), unlocked: Arc::default() }
    }
}

/// Leave to sync without the cross-process lock, until the app leaves the front.
pub(crate) struct Unlocked {
    _permit: OwnedRwLockReadGuard<()>,
    since: watch::Receiver<Option<Instant>>,
}

impl Unlocked {
    /// Resolves once the app left the front: the sync must stop.
    pub(crate) async fn left(&mut self) {
        let _ = self.since.wait_for(Option::is_none).await;
    }
}

impl Front {
    /// The app came to the front or left it, now. The returned future finishes once no sync
    /// runs without the lock (at once when coming to the front).
    pub(crate) fn set(&self, front: bool) -> impl Future<Output = ()> + Send + 'static {
        self.since.send_if_modified(|since| match (front, since.is_some()) {
            (true, false) => {
                *since = Some(Instant::now());
                true
            }
            (false, true) => {
                *since = None;
                true
            }
            _ => false,
        });
        let unlocked = Arc::clone(&self.unlocked);
        async move {
            if !front {
                drop(unlocked.write().await);
            }
        }
    }

    /// Leave to sync without the lock, when the app has been in front longer than a push may
    /// run.
    pub(crate) async fn unlocked(&self) -> Option<Unlocked> {
        // Taken before looking, so leaving the front (which changes the state, then waits for
        // the permits) either is seen here or waits for this one.
        let permit = Arc::clone(&self.unlocked).read_owned().await;
        let mut since = self.since.subscribe();
        let long_enough = since.borrow_and_update().is_some_and(|at| at.elapsed() >= PUSH_MAY_RUN);
        long_enough.then_some(Unlocked { _permit: permit, since })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn an_engine_never_told_should_sync_under_the_lock() {
        assert!(Front::default().unlocked().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn an_app_just_in_front_should_sync_under_the_lock() {
        let front = Front::default();
        front.set(true).await;
        tokio::time::advance(PUSH_MAY_RUN - Duration::from_secs(1)).await;
        assert!(front.unlocked().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn an_app_in_front_longer_than_a_push_runs_should_sync_without_the_lock() {
        let front = Front::default();
        front.set(true).await;
        tokio::time::advance(PUSH_MAY_RUN).await;
        assert!(front.unlocked().await.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn leaving_the_front_should_wait_for_a_sync_without_the_lock_to_stop() {
        let front = Front::default();
        front.set(true).await;
        tokio::time::advance(PUSH_MAY_RUN).await;
        let mut sync = front.unlocked().await.expect("unlocked");
        let mut leaving = Box::pin(front.set(false));
        assert!(futures_util::poll!(&mut leaving).is_pending(), "a sync still runs without the lock");
        sync.left().await;
        drop(sync);
        leaving.await;
    }

    #[tokio::test(start_paused = true)]
    async fn coming_back_to_the_front_should_start_the_wait_for_pushes_again() {
        let front = Front::default();
        front.set(true).await;
        tokio::time::advance(PUSH_MAY_RUN).await;
        front.set(false).await;
        front.set(true).await;
        assert!(front.unlocked().await.is_none());
    }
}
