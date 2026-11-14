//! Invitations and public rooms: results and actions always retain their account and space.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Directory {
    Loading,
    Ready(Vec<matrix_core::PublicRoom>),
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipAction {
    JoinRoom,
    JoinSpace,
    Decline,
}

impl State {
    pub fn load_public_rooms(&mut self, account: AccountId, space: String) {
        self.directories.insert((account.clone(), space.clone()), Directory::Loading);
        self.dispatch(
            Command::ListPublicRooms { account: account.clone(), space: space.clone() },
            Purpose::Directory { account, space },
        );
    }

    pub(super) fn discover_scope(&mut self) {
        let (Some(account), Scope::Space(space)) = (self.nav.account.clone(), self.active_scope()) else { return };
        if !self.directories.contains_key(&(account.clone(), space.clone())) {
            self.load_public_rooms(account, space);
        }
    }

    pub fn browse_public_rooms(&mut self) {
        let (Some(account), Scope::Space(space)) = (self.nav.account.clone(), self.active_scope()) else { return };
        self.discover_scope();
        self.modal = Some(Modal::PublicRooms { account, space });
    }

    pub fn show_invitation(&mut self, room_id: &str) {
        let Some(account) = self.nav.account.clone() else { return };
        if self
            .accounts
            .get(&account)
            .is_some_and(|data| data.invitations.iter().any(|invite| invite.room_id == room_id))
        {
            self.modal = Some(Modal::Invitation { account, room_id: room_id.to_owned() });
        }
    }

    pub fn membership_pending(&self, account: &AccountId, room_id: &str) -> bool {
        self.pending.values().any(|purpose| matches!(purpose, Purpose::Membership { account: owner, room_id: room, .. } if owner == account && room == room_id))
    }

    pub fn respond_to_invitation(&mut self, account: &AccountId, room_id: &str, accept: bool) {
        let Some(invite) = self
            .accounts
            .get(account)
            .and_then(|data| data.invitations.iter().find(|invite| invite.room_id == room_id))
        else {
            return;
        };
        let action = match (accept, invite.space) {
            (true, true) => MembershipAction::JoinSpace,
            (true, false) => MembershipAction::JoinRoom,
            (false, _) => MembershipAction::Decline,
        };
        self.change_membership(account, room_id, Vec::new(), action);
    }

    pub fn join_public_room(&mut self, account: &AccountId, space: &str, room_id: &str) {
        let Some(Directory::Ready(rooms)) = self.directories.get(&(account.clone(), space.to_owned())) else { return };
        let Some(room) = rooms.iter().find(|room| room.room_id == room_id) else { return };
        self.change_membership(account, room_id, room.via.clone(), MembershipAction::JoinRoom);
    }

    fn change_membership(&mut self, account: &AccountId, room_id: &str, via: Vec<String>, action: MembershipAction) {
        if self.membership_pending(account, room_id) {
            return;
        }
        let command = match action {
            MembershipAction::Decline => {
                Command::LeaveRoom { account: account.clone(), room_id: room_id.to_owned(), forget: false }
            }
            _ => Command::JoinRoom { account: account.clone(), room_id: room_id.to_owned(), via },
        };
        self.dispatch(
            command,
            Purpose::Membership {
                account: account.clone(),
                room_id: room_id.to_owned(),
                action,
                scope: self.active_scope(),
            },
        );
    }

    pub(super) fn membership_finished(
        &mut self,
        account: &AccountId,
        room_id: &str,
        action: MembershipAction,
        scope: &Scope,
    ) {
        self.accounts.forget_invitation(account, room_id);
        let invitation_open = matches!(&self.modal, Some(Modal::Invitation { account: owner, room_id: room }) if owner == account && room == room_id);
        let directory_open = matches!(&self.modal, Some(Modal::PublicRooms { account: owner, space }) if owner == account && *scope == Scope::Space(space.clone()));
        if invitation_open || directory_open {
            self.modal = None;
        }
        let still_here = self.nav.account.as_ref() == Some(account)
            && self.active_scope() == *scope
            && (invitation_open || directory_open || self.active_room_id().is_none());
        if still_here {
            match action {
                MembershipAction::JoinSpace => self.select_scope(Scope::Space(room_id.to_owned())),
                MembershipAction::JoinRoom => self.open_room(account, room_id),
                MembershipAction::Decline => {}
            }
        }
    }
}
