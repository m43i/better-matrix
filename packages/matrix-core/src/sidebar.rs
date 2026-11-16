//! How the user arranges the sidebar: sections of their own, which rooms and chats they hold,
//! and which sections are collapsed, for Home and for each space; and the order of the spaces
//! on the rail. Kept in the account's data on
//! the homeserver, so every session shows the same arrangement.

use std::collections::BTreeMap;

use matrix_sdk::ruma::events::GlobalAccountDataEventType;
use matrix_sdk::ruma::serde::Raw;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::account::Account;

/// The account data event the layout is stored in.
const EVENT_TYPE: &str = "chat.bettermatrix.sidebar";

/// The sidebar's arrangement, by scope: `home`, or a space's room ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub struct SidebarLayout {
    /// Each scope's arrangement, by `home` or the space's room ID.
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeLayout>,
    /// The order of the spaces on the rail, by room ID. Spaces it doesn't name come after the
    /// ones it does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spaces: Vec<String>,
    /// When it was changed, in milliseconds since the Unix epoch: of two layouts, the later
    /// one wins, so a sync delivering an older save never undoes a newer one.
    #[serde(default)]
    pub changed_at: u64,
}

/// One scope's arrangement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub struct ScopeLayout {
    /// Sections the user collapsed: the built-in `rooms` and `direct`, or a section's ID.
    #[serde(default)]
    pub collapsed: Vec<String>,
    /// The user's own sections.
    #[serde(default)]
    pub sections: Vec<Section>,
    /// The order the sections show in, built-in ones (`rooms`, `direct`) included. Sections it
    /// doesn't name come after the ones it does.
    #[serde(default)]
    pub order: Vec<String>,
}

/// A section the user made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(ts_rs::TS))]
pub struct Section {
    /// Stable within the scope.
    pub id: String,
    /// What the user called it.
    pub name: String,
    /// The rooms and direct chats in it, in order.
    #[serde(default)]
    pub rooms: Vec<String>,
}

/// Whether a sync's account data carries a new layout.
pub(crate) fn changed_in(account_data: &[Raw<matrix_sdk::ruma::events::AnyGlobalAccountDataEvent>]) -> bool {
    account_data.iter().any(|raw| raw.get_field::<String>("type").ok().flatten().as_deref() == Some(EVENT_TYPE))
}

impl Account {
    /// The saved layout: from this device's copy of the account data, or the server's; empty
    /// when none was saved yet or it can't be read.
    pub async fn sidebar_layout(&self) -> Result<SidebarLayout, Error> {
        let account = self.client().account();
        let event_type = GlobalAccountDataEventType::from(EVENT_TYPE);
        let raw = match account.account_data_raw(event_type.clone()).await? {
            Some(raw) => Some(raw),
            None => account.fetch_account_data(event_type).await?,
        };
        Ok(raw.and_then(|raw| serde_json::from_str::<SidebarLayout>(raw.json().get()).ok()).unwrap_or_default())
    }

    /// Saves the layout to the account's data.
    pub async fn set_sidebar_layout(&self, layout: &SidebarLayout) -> Result<(), Error> {
        let raw = Raw::new(layout).map_err(|e| Error::Storage(e.into()))?.cast_unchecked();
        self.client().account().set_account_data_raw(GlobalAccountDataEventType::from(EVENT_TYPE), raw).await?;
        Ok(())
    }
}
