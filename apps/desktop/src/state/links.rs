//! Permalinks opened in the app: matrix.to links and `matrix:` URIs, from a message, from the OS
//! (a browser, another app) or from the app's launch.
//!
//! A room one of the accounts has joined opens at once ([`State::open_room`]); the desktop has
//! no jump to a message, so a link to one opens its room. Anything else opens the link dialog
//! ([`Modal::Link`]): a room's preview with Join, or a person with Start chat, for the account
//! on screen, switchable when several are signed in. A link that comes before any account is
//! ready waits for the first room list.

use matrix_core::{AccountId, Command, LinkTarget, Permalink, Person, RequestId, RoomPreview};

use super::{Modal, Purpose, Scope, Screen, State};
use crate::i18n::{t, tf};

/// The link dialog: what the link names, the account it acts for, and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkDialog {
    pub link: Permalink,
    /// The account the dialog previews and joins with.
    pub account: AccountId,
    /// The user picked the account: a preview no longer opens the room in another account that
    /// has joined it.
    pub chosen: bool,
    pub step: LinkStep,
}

/// Where the link dialog stands.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkStep {
    /// The room's preview is on its way.
    Previewing(RequestId),
    /// The room as it looks from outside.
    Preview(Box<RoomPreview>),
    /// No server would describe the room; joining may still work.
    NoPreview,
    /// Joining, from the preview shown (if there was one).
    Joining { request: RequestId, preview: Option<Box<RoomPreview>> },
    /// A person, their profile on its way while `loading`.
    Person { person: Person, request: Option<RequestId> },
}

impl LinkDialog {
    /// The preview shown, if any.
    pub fn preview(&self) -> Option<&RoomPreview> {
        match &self.step {
            LinkStep::Preview(preview) | LinkStep::Joining { preview: Some(preview), .. } => Some(preview),
            _ => None,
        }
    }
}

impl State {
    /// Opens a permalink; anything that isn't one is reported and dropped. Before an account is
    /// ready, the link waits for the first room list.
    pub fn open_permalink(&mut self, link: &str) {
        let Some(permalink) = Permalink::parse(link) else {
            self.notice = Some(t!("link.invalid").to_owned());
            return;
        };
        self.follow(permalink);
    }

    /// Opens a link or address typed or pasted into the top bar's search
    /// ([`Permalink::parse_typed`]): `#rust:matrix.org`, a user ID, an invitation's matrix.to
    /// link. Anything else stays a search.
    pub fn open_typed_link(&mut self, text: &str) {
        let Some(permalink) = Permalink::parse_typed(text) else { return };
        self.reset_search();
        self.follow(permalink);
    }

    /// Opens a parsed permalink, or keeps it until an account is ready.
    fn follow(&mut self, permalink: Permalink) {
        let Some(account) = self.nav.account.clone().filter(|_| self.screen() == Screen::Chat) else {
            self.pending_link = Some(permalink);
            return;
        };
        self.popover = None;
        if let LinkTarget::Room { room, event } = &permalink.target
            && let Some(joined) = self.joined_account(room, &account)
        {
            self.modal = None;
            self.open_linked(&joined, room, false);
            // A link to a message goes on to it, wherever it shows (the room, or a thread).
            if let Some(event) = event {
                self.jump_to_message(&joined, room, event);
            }
            return;
        }
        self.modal =
            Some(Modal::Link(LinkDialog { link: permalink, account, chosen: false, step: LinkStep::NoPreview }));
        self.look_up_link();
    }

    /// The app was launched again while it runs: opens the permalinks that launch handed on.
    pub fn take_handed_links(&self) {
        self.shell.take_handed_links();
    }

    /// Opens the link that waited for an account, once one is ready.
    pub(super) fn open_pending_link(&mut self) {
        if self.screen() != Screen::Chat || self.nav.account.is_none() {
            return;
        }
        if let Some(link) = self.pending_link.take() {
            self.follow(link);
        }
    }

    /// The dialog acts for another account: it looks the room or person up again for it.
    pub fn choose_link_account(&mut self, account: &AccountId) {
        let Some(Modal::Link(dialog)) = &mut self.modal else { return };
        if dialog.account == *account || matches!(dialog.step, LinkStep::Joining { .. }) {
            return;
        }
        dialog.account = account.clone();
        dialog.chosen = true;
        self.look_up_link();
    }

    /// Joins the linked room with the dialog's account.
    pub fn join_linked_room(&mut self) {
        let Some(Modal::Link(dialog)) = &self.modal else { return };
        let preview = match &dialog.step {
            LinkStep::Preview(preview) if preview.joinable() => Some(preview.clone()),
            LinkStep::NoPreview => None,
            _ => return,
        };
        let LinkTarget::Room { room, .. } = &dialog.link.target else { return };
        let room = preview.as_ref().map_or_else(|| room.clone(), |preview| preview.room_id.clone());
        let command =
            Command::JoinRoom { account: dialog.account.clone(), room_id: room, via: dialog.link.via.clone() };
        let request = self.dispatch(command, Purpose::Permalink);
        if let Some(Modal::Link(dialog)) = &mut self.modal {
            dialog.step = LinkStep::Joining { request, preview };
        }
    }

    /// Opens (or starts) the direct chat with the linked person, in the dialog's account.
    pub fn chat_with_linked_person(&mut self) {
        let Some(Modal::Link(LinkDialog { account, step: LinkStep::Person { person, .. }, .. })) = &self.modal else {
            return;
        };
        let (account, user_id) = (account.clone(), person.id.clone());
        self.modal = None;
        self.select_account(&account);
        self.dispatch(Command::OpenDirectChat { account: account.clone(), user_id }, Purpose::DirectChat(account));
    }

    /// Asks for what the dialog shows: the room's preview, or the person's profile (through the
    /// user directory, which fetches a full user ID's profile from its server).
    fn look_up_link(&mut self) {
        let Some(Modal::Link(dialog)) = &self.modal else { return };
        let account = dialog.account.clone();
        let command = match &dialog.link.target {
            LinkTarget::Room { room, .. } => {
                Command::PreviewRoom { account, room: room.clone(), via: dialog.link.via.clone() }
            }
            LinkTarget::User(user_id) => Command::SearchPeople { account, term: user_id.clone() },
        };
        let request = self.dispatch(command, Purpose::Permalink);
        if let Some(Modal::Link(dialog)) = &mut self.modal {
            dialog.step = match &dialog.link.target {
                LinkTarget::Room { .. } => LinkStep::Previewing(request),
                LinkTarget::User(user_id) => LinkStep::Person {
                    person: Person { id: user_id.clone(), name: user_id.clone(), avatar: None },
                    request: Some(request),
                },
            };
        }
        self.request_avatars();
    }

    /// Opens a linked room, or a linked space as the account's scope: known as one, or
    /// `space` by its preview while the room list doesn't say yet.
    fn open_linked(&mut self, account: &AccountId, room_id: &str, space: bool) {
        if space || self.accounts.get(account).is_some_and(|data| data.space(room_id).is_some()) {
            self.select_account(account);
            self.select_scope(Scope::Space(room_id.to_owned()));
        } else {
            self.open_room(account, room_id);
        }
    }

    /// The account that has joined the room or space `room_id`: `preferred` when it has, else
    /// the first that has.
    fn joined_account(&self, room_id: &str, preferred: &AccountId) -> Option<AccountId> {
        let joined = |id: &AccountId| {
            self.accounts.get(id).is_some_and(|data| data.room(room_id).is_some() || data.space(room_id).is_some())
        };
        if joined(preferred) {
            return Some(preferred.clone());
        }
        self.accounts.iter().map(|data| data.summary.id.clone()).find(joined)
    }

    /// A room's preview came: a room already joined opens, unless the user picked an account
    /// that hasn't joined it.
    pub(super) fn link_previewed(&mut self, request: RequestId, preview: RoomPreview) {
        let Some(Modal::Link(dialog)) = &self.modal else { return };
        if dialog.step != LinkStep::Previewing(request) {
            return;
        }
        let account = dialog.account.clone();
        let event = linked_event(&dialog.link);
        let opens = if preview.joined {
            Some(account)
        } else if dialog.chosen {
            None
        } else {
            self.joined_account(&preview.room_id, &account)
        };
        if let Some(account) = opens {
            self.modal = None;
            self.open_linked(&account, &preview.room_id, preview.space);
            if let Some(event) = event {
                self.jump_to_message(&account, &preview.room_id, &event);
            }
            return;
        }
        if let Some(Modal::Link(dialog)) = &mut self.modal {
            dialog.step = LinkStep::Preview(Box::new(preview));
        }
        self.request_avatars();
    }

    /// The linked person's profile came, among the directory's results.
    pub(super) fn link_person_found(&mut self, request: RequestId, people: &[Person]) {
        let Some(Modal::Link(LinkDialog { step: LinkStep::Person { person, request: waiting }, .. })) = &mut self.modal
        else {
            return;
        };
        if *waiting != Some(request) {
            return;
        }
        *waiting = None;
        if let Some(found) = people.iter().find(|found| found.id == person.id) {
            *person = found.clone();
        }
        self.request_avatars();
    }

    /// The room or space joined: it opens, if the dialog still waits for it, at the linked
    /// message if the link names one.
    pub(super) fn link_joined(&mut self, request: RequestId, room_id: &str) {
        let Some(Modal::Link(dialog)) = &self.modal else { return };
        let LinkStep::Joining { request: waiting, preview } = &dialog.step else { return };
        if *waiting != request {
            return;
        }
        let space = preview.as_ref().is_some_and(|preview| preview.space);
        let (account, event) = (dialog.account.clone(), linked_event(&dialog.link));
        self.modal = None;
        self.open_linked(&account, room_id, space);
        if let Some(event) = event {
            self.jump_to_message(&account, room_id, &event);
        }
    }

    /// A lookup or join of the dialog failed: a preview that couldn't load still offers to join;
    /// a join that failed says why and goes back; a profile that couldn't load shows the ID.
    pub(super) fn link_failed(&mut self, request: RequestId, error: String) {
        let Some(Modal::Link(dialog)) = &mut self.modal else { return };
        match &mut dialog.step {
            LinkStep::Previewing(waiting) if *waiting == request => dialog.step = LinkStep::NoPreview,
            LinkStep::Joining { request: waiting, preview } if *waiting == request => {
                dialog.step = preview.take().map_or(LinkStep::NoPreview, LinkStep::Preview);
                self.notice = Some(tf!("notice.join", error = error));
            }
            LinkStep::Person { request: waiting, .. } if *waiting == Some(request) => *waiting = None,
            _ => {}
        }
    }

    /// Whether the link dialog's room preview shows initials rather than the room's picture: the
    /// account is invited there (as the preview says, or as its invitations hold the room) and
    /// hides invitations' pictures (ADR 0011).
    pub fn link_hides_picture(&self, dialog: &LinkDialog, preview: &RoomPreview) -> bool {
        self.accounts.get(&dialog.account).is_some_and(|data| {
            let invited = preview.invited || data.invitations.iter().any(|invite| invite.room_id == preview.room_id);
            invited && !data.safety.invite_pictures
        })
    }

    /// The pictures the link dialog shows: the room's or the person's, from its account.
    pub(super) fn link_pictures(&self) -> Option<(AccountId, matrix_core::MediaRef)> {
        let Some(Modal::Link(dialog)) = &self.modal else { return None };
        let picture = match &dialog.step {
            LinkStep::Person { person, .. } => person.avatar.clone(),
            _ => dialog
                .preview()
                .filter(|preview| !self.link_hides_picture(dialog, preview))
                .and_then(|preview| preview.avatar.clone()),
        }?;
        Some((dialog.account.clone(), picture))
    }
}

/// The message a link names in its room, if it names one.
fn linked_event(link: &Permalink) -> Option<String> {
    match &link.target {
        LinkTarget::Room { event, .. } => event.clone(),
        LinkTarget::User(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use matrix_core::{
        AccountSummary, DetachedCommands, EngineHandle, Event, NotificationMode, RoomKind, RoomList, RoomSummary,
    };
    use rune::Tx;

    use super::*;
    use crate::platform::Shell;
    use crate::state::Settings;

    const ADA: &str = "@ada:example.org";
    const HOBBY: &str = "@ada:hobby.chat";

    fn state() -> (State, DetachedCommands) {
        let (engine, commands) = EngineHandle::detached();
        (State::new(engine, Shell::headless(), Settings::fixed_english()), commands)
    }

    fn apply(s: &mut State, event: Event) {
        s.apply(event, &mut Tx::default());
    }

    fn signed_in(s: &mut State, user_id: &str, rooms: &[&str]) {
        apply(
            s,
            Event::AccountAdded(AccountSummary {
                id: AccountId::new(user_id),
                display_name: user_id.into(),
                homeserver: "example.org".into(),
                default_notifications: NotificationMode::AllMessages,
                avatar: None,
                sign_in: matrix_core::SignInMethod::Password,
            }),
        );
        let rooms = rooms
            .iter()
            .map(|room_id| RoomSummary {
                room_id: (*room_id).into(),
                name: (*room_id).into(),
                topic: None,
                kind: RoomKind::Group,
                unread: 0,
                mentions: 0,
                dm_users: Vec::new(),
                note_to_self: false,
                avatar: None,
                member_count: 2,
                active_member_count: 2,
                history: matrix_core::HistoryVisibility::Shared,
                notifications: NotificationMode::AllMessages,
                custom_notifications: false,
                call: matrix_core::RoomCall::default(),
                activity_ms: None,
                encrypted: false,
                public: false,
                can_rename: false,
                pinned: Vec::new(),
                can_pin: true,
                saved: Vec::new(),
                priority: matrix_core::RoomPriority::Normal,
                marked_unread: false,
                can_change_settings: false,
                knocks: 0,
                successor: None,
            })
            .collect();
        apply(
            s,
            Event::RoomsUpdated {
                account: AccountId::new(user_id),
                list: RoomList { rooms, spaces: Vec::new(), invitations: Vec::new() },
            },
        );
    }

    /// Signs `user_id` in with the spaces `spaces` and no rooms.
    fn in_spaces(s: &mut State, user_id: &str, spaces: &[&str]) {
        signed_in(s, user_id, &[]);
        let spaces = spaces
            .iter()
            .map(|room_id| matrix_core::SpaceSummary {
                room_id: (*room_id).into(),
                name: (*room_id).into(),
                avatar: None,
                can_add_rooms: false,
                can_rename: false,
                can_change_settings: false,
                children: Vec::new(),
                members: Vec::new(),
            })
            .collect();
        apply(
            s,
            Event::RoomsUpdated {
                account: AccountId::new(user_id),
                list: RoomList { rooms: Vec::new(), spaces, invitations: Vec::new() },
            },
        );
    }

    fn scope(s: &State) -> Option<&Scope> {
        s.nav.account.as_ref().and_then(|account| s.nav.scopes.get(account))
    }

    fn commands(commands: &mut DetachedCommands) -> Vec<(RequestId, Command)> {
        std::iter::from_fn(|| commands.try_next()).filter(|(_, c)| !matches!(c, Command::ListMembers { .. })).collect()
    }

    fn preview(room_id: &str) -> RoomPreview {
        RoomPreview {
            room_id: room_id.into(),
            alias: Some("#rust:matrix.org".into()),
            name: Some("Rust".into()),
            topic: None,
            avatar: None,
            member_count: 1200,
            join_rule: Some(matrix_core::JoinRule::Public),
            space: false,
            joined: false,
            invited: false,
        }
    }

    fn step(s: &State) -> Option<&LinkStep> {
        match &s.modal {
            Some(Modal::Link(dialog)) => Some(&dialog.step),
            _ => None,
        }
    }

    /// Opens `link` on Ada's work account and answers its preview with `answer`.
    fn previewed(link: &str, answer: RoomPreview) -> (State, DetachedCommands) {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        commands(&mut sent);
        s.open_permalink(link);
        let (request, _) = commands(&mut sent).into_iter().next().unwrap();
        apply(&mut s, Event::RoomPreview { request, preview: Box::new(answer) });
        (s, sent)
    }

    #[test]
    fn a_link_to_a_joined_room_should_open_it_without_a_dialog() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &["!lab:example.org"]);
        s.open_permalink("https://matrix.to/#/!lab:example.org/$event");
        assert_eq!((s.active_room_id(), s.modal.is_none()), (Some("!lab:example.org"), true));
    }

    #[test]
    fn a_typed_alias_should_ask_for_its_rooms_preview() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        commands(&mut sent);
        s.open_typed_link(" #rust:matrix.org ");
        let sent = commands(&mut sent);
        assert!(matches!(sent.as_slice(), [(_, Command::PreviewRoom { room, .. })] if room == "#rust:matrix.org"));
    }

    #[test]
    fn a_typed_room_id_of_a_joined_room_should_open_it_and_clear_the_search() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &["!lab:example.org"]);
        s.search.field = rune::TextEdit::new("!lab:example.org");
        s.open_typed_link("!lab:example.org");
        assert_eq!((s.active_room_id(), s.search.query()), (Some("!lab:example.org"), ""));
    }

    #[test]
    fn a_typed_name_should_stay_a_search() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &[]);
        s.search.field = rune::TextEdit::new("rust");
        s.open_typed_link("rust");
        assert_eq!((s.modal.is_none(), s.search.query()), (true, "rust"));
    }

    #[test]
    fn a_link_to_a_room_another_account_joined_should_open_it_there() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &[]);
        signed_in(&mut s, HOBBY, &["!garden:hobby.chat"]);
        s.select_account(&AccountId::new(ADA));
        s.open_permalink("matrix:roomid/garden:hobby.chat");
        assert_eq!(s.nav.account, Some(AccountId::new(HOBBY)));
        assert_eq!(s.active_room_id(), Some("!garden:hobby.chat"));
    }

    #[test]
    fn a_link_to_a_joined_space_should_show_the_space() {
        let (mut s, _) = state();
        in_spaces(&mut s, ADA, &["!guild:example.org"]);
        s.open_permalink("https://matrix.to/#/!guild:example.org");
        assert_eq!((scope(&s), s.modal.is_none()), (Some(&Scope::Space("!guild:example.org".into())), true));
    }

    #[test]
    fn joining_a_linked_space_should_show_the_space() {
        let space = RoomPreview { space: true, ..preview("!guild:example.org") };
        let (mut s, mut sent) = previewed("https://matrix.to/#/#guild:example.org", space);
        s.join_linked_room();
        let (request, _) = commands(&mut sent).into_iter().next().unwrap();
        apply(&mut s, Event::RoomJoined { request, room_id: "!guild:example.org".into() });
        assert_eq!((scope(&s), s.active_room_id()), (Some(&Scope::Space("!guild:example.org".into())), None));
    }

    #[test]
    fn a_link_to_an_unknown_room_should_ask_for_its_preview_with_the_links_servers() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        commands(&mut sent);
        s.open_permalink("https://matrix.to/#/%23rust%3Amatrix.org?via=matrix.org");
        let sent = commands(&mut sent);
        assert!(matches!(
            sent.as_slice(),
            [(_, Command::PreviewRoom { room, via, .. })] if room == "#rust:matrix.org" && via == &["matrix.org"]
        ));
        assert!(matches!(step(&s), Some(LinkStep::Previewing(_))));
    }

    #[test]
    fn a_preview_should_show_in_the_dialog() {
        let (s, _) = previewed("https://matrix.to/#/#rust:matrix.org", preview("!rust:matrix.org"));
        assert!(matches!(step(&s), Some(LinkStep::Preview(shown)) if shown.room_id == "!rust:matrix.org"));
    }

    #[test]
    fn a_preview_of_a_room_the_account_joined_should_open_it() {
        let joined = RoomPreview { joined: true, ..preview("!rust:matrix.org") };
        let (s, _) = previewed("https://matrix.to/#/#rust:matrix.org", joined);
        assert_eq!((s.active_room_id(), s.modal.is_none()), (Some("!rust:matrix.org"), true));
    }

    #[test]
    fn a_failed_preview_should_still_offer_to_join() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        commands(&mut sent);
        s.open_permalink("matrix:r/secret:example.org");
        let (request, _) = commands(&mut sent).into_iter().next().unwrap();
        apply(&mut s, Event::Finished { request, result: Err("forbidden".into()) });
        s.join_linked_room();
        assert!(matches!(
            commands(&mut sent).as_slice(),
            [(_, Command::JoinRoom { room_id: room, .. })] if room == "#secret:example.org"
        ));
    }

    #[test]
    fn joining_should_use_the_previewed_room_id_and_open_the_room_once_joined() {
        let (mut s, mut sent) =
            previewed("https://matrix.to/#/#rust:matrix.org?via=matrix.org", preview("!rust:matrix.org"));
        s.join_linked_room();
        let (request, command) = commands(&mut sent).into_iter().next().unwrap();
        assert!(
            matches!(command, Command::JoinRoom { room_id: room, via, .. } if room == "!rust:matrix.org" && via == ["matrix.org"])
        );
        apply(&mut s, Event::RoomJoined { request, room_id: "!rust:matrix.org".into() });
        assert_eq!((s.active_room_id(), s.modal.is_none()), (Some("!rust:matrix.org"), true));
    }

    #[test]
    fn a_failed_join_should_go_back_to_the_preview_and_say_why() {
        let (mut s, mut sent) = previewed("https://matrix.to/#/#rust:matrix.org", preview("!rust:matrix.org"));
        s.join_linked_room();
        let (request, _) = commands(&mut sent).into_iter().next().unwrap();
        apply(&mut s, Event::Finished { request, result: Err("banned".into()) });
        assert!(matches!(step(&s), Some(LinkStep::Preview(_))));
        assert_eq!(s.notice.as_deref(), Some("Could not join: banned"));
    }

    #[test]
    fn an_invite_only_room_should_not_be_joined() {
        let closed = RoomPreview { join_rule: Some(matrix_core::JoinRule::Invite), ..preview("!club:example.org") };
        let (mut s, mut sent) = previewed("matrix:roomid/club:example.org", closed);
        s.join_linked_room();
        assert!(commands(&mut sent).is_empty());
    }

    #[test]
    fn choosing_another_account_should_preview_the_room_for_it() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, HOBBY, &[]);
        signed_in(&mut s, ADA, &[]);
        s.select_account(&AccountId::new(ADA));
        commands(&mut sent);
        s.open_permalink("https://matrix.to/#/#rust:matrix.org");
        s.choose_link_account(&AccountId::new(HOBBY));
        let sent = commands(&mut sent);
        assert!(matches!(sent.last(), Some((_, Command::PreviewRoom { account, .. })) if account.as_str() == HOBBY));
    }

    #[test]
    fn a_link_to_a_person_should_look_up_their_profile() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        commands(&mut sent);
        s.open_permalink("matrix:u/grace:example.org?action=chat");
        let (request, command) = commands(&mut sent).into_iter().next().unwrap();
        assert!(matches!(command, Command::SearchPeople { term, .. } if term == "@grace:example.org"));
        let grace = Person { id: "@grace:example.org".into(), name: "Grace Hopper".into(), avatar: None };
        let other = Person { id: "@grace:other.org".into(), name: "Another Grace".into(), avatar: None };
        apply(&mut s, Event::PeopleFound { request, people: vec![other, grace] });
        assert!(matches!(step(&s), Some(LinkStep::Person { person, request: None }) if person.name == "Grace Hopper"));
    }

    #[test]
    fn starting_a_chat_with_a_linked_person_should_open_their_direct_chat() {
        let (mut s, mut sent) = state();
        signed_in(&mut s, ADA, &[]);
        s.open_permalink("https://matrix.to/#/@grace:example.org");
        commands(&mut sent);
        s.chat_with_linked_person();
        assert!(matches!(
            commands(&mut sent).as_slice(),
            [(_, Command::OpenDirectChat { user_id, .. })] if user_id == "@grace:example.org"
        ));
        assert!(s.modal.is_none());
    }

    #[test]
    fn a_link_before_any_account_is_ready_should_open_with_the_first_room_list() {
        let (mut s, _) = state();
        s.open_permalink("https://matrix.to/#/!lab:example.org");
        assert!(s.modal.is_none());
        signed_in(&mut s, ADA, &["!lab:example.org"]);
        assert_eq!(s.active_room_id(), Some("!lab:example.org"));
    }

    #[test]
    fn something_that_is_not_a_permalink_should_be_reported() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &[]);
        s.open_permalink("https://example.org/#/@grace:example.org");
        assert_eq!((s.modal.is_none(), s.notice.is_some()), (true, true));
    }

    #[test]
    fn a_matrix_to_link_in_a_message_should_open_in_the_app() {
        let (mut s, _) = state();
        signed_in(&mut s, ADA, &["!lab:example.org"]);
        s.open_link("https://matrix.to/#/%21lab%3Aexample.org", &mut Tx::default());
        assert_eq!(s.active_room_id(), Some("!lab:example.org"));
    }
}
