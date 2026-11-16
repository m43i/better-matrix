use matrix_core::AccountId;
use rune::Tx;

use super::{Scope, Screen, State};

/// Run a UI action and land a changed destination in the same commit as the input.
pub fn navigate(action: impl Fn(&mut State)) -> impl Fn(&mut State, &mut Tx) {
    move |state, tx| {
        let location = Location::of(state);
        action(state);
        if !location.is_current(state) {
            tx.settle();
        }
    }
}

/// The main pane's destination. Threads and side panels can animate within it.
#[derive(Debug)]
pub(super) struct Location {
    screen: Screen,
    account: Option<AccountId>,
    scope: Scope,
    room: Option<String>,
}

impl Location {
    pub fn of(state: &State) -> Self {
        Self {
            screen: state.screen(),
            account: state.nav.account.clone(),
            scope: state.active_scope(),
            room: state.active_room_id().map(str::to_owned),
        }
    }

    pub fn is_current(&self, state: &State) -> bool {
        let scope = state.nav.account.as_ref().and_then(|account| state.nav.scopes.get(account));
        self.screen == state.screen()
            && self.account == state.nav.account
            && &self.scope == scope.unwrap_or(&Scope::Home)
            && self.room.as_deref() == state.active_room_id()
    }
}
