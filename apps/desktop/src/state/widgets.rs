//! Running a room's widgets, its meetings and the integration manager: a widget run
//! ([`Command::RunWidget`], [`Command::StartMeeting`], [`Command::OpenIntegrations`]) shows its
//! page in the browser ([`Event::OpenUrl`](matrix_core::Event::OpenUrl)) or in a widget window
//! of its own ([`crate::platform::widget_window`]), passing the page's messages both ways, until
//! the window closes ([`Command::Cancel`]) or the engine finishes the run (the window closes).
//!
//! - Meetings open in the browser, where WebRTC and the meeting servers' sign-ins work; other
//!   widgets in the widget window, or the browser when asked.
//! - What a widget asks to do waits for the user, as do the integration manager's terms
//!   ([`WidgetPrompt`]): queued until answered or their run ends, the oldest shown
//!   ([`Modal::WidgetPrompt`]) whenever no other dialog shows.

use matrix_core::{AccountId, Answer, Command, Policy, RequestId, Widget, WidgetAccess, WidgetPage, WidgetPermission};

use super::{Modal, Popover, Purpose, State};
use crate::i18n::{t, tf};

/// What a running widget asks to be allowed to do, each ticked to grant it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetPermissions {
    /// The run that asks.
    pub request: RequestId,
    /// The question's number within the run, which the answer names.
    pub question: u64,
    /// The widget's name.
    pub widget: String,
    /// What it asks for, in the order asked.
    pub permissions: Vec<WidgetPermission>,
    /// The destination and account shown before opening.
    pub context: Option<WidgetAccess>,
    /// Which of them the user grants, by index.
    pub granted: Vec<bool>,
}

impl WidgetPermissions {
    /// Which question this is ([`PromptKey`]).
    pub fn key(&self) -> PromptKey {
        PromptKey { request: self.request, question: Some(self.question) }
    }
}

impl State {
    /// The meetings pinned to the room shown, by name.
    pub fn meetings_here(&self) -> Vec<&Widget> {
        let (Some(account), Some(room_id)) = (self.active_account(), self.active_room_id()) else {
            return Vec::new();
        };
        account
            .widgets
            .get(room_id)
            .map(|listed| listed.widgets.iter().filter(|widget| widget.meeting.is_some()).collect())
            .unwrap_or_default()
    }

    /// The room shown and whose it is, for a command about it.
    fn widget_room(&self) -> Option<(AccountId, String)> {
        Some((self.nav.account.clone()?, self.active_room_id()?.to_owned()))
    }

    /// Runs one of the shown room's widgets: in the browser, or in a widget window. The run's
    /// request, unless no room shows.
    pub fn run_widget(&mut self, widget_id: &str, browser: bool) -> Option<RequestId> {
        self.popover = None;
        let (account, room_id) = self.widget_room()?;
        let command = Command::RunWidget {
            account: account.clone(),
            room_id: room_id.clone(),
            widget_id: widget_id.to_owned(),
            browser,
        };
        Some(self.launch_widget(account, room_id, command))
    }

    fn launch_widget(&mut self, account: AccountId, room: String, command: Command) -> RequestId {
        if let Some(data) = self.accounts.get_mut(&account) {
            data.widget_launch_errors.remove(&room);
        }
        let request = self.dispatch(command.clone(), Purpose::WidgetRun { closed: false });
        self.widget_launches.insert(request, (account, room, command));
        request
    }

    /// Retries the failed launch for the integrations panel showing.
    pub fn retry_widget(&mut self) {
        let Some((account, room)) = self.widget_room() else { return };
        if let Some((_, command)) =
            self.accounts.get_mut(&account).and_then(|data| data.widget_launch_errors.remove(&room))
        {
            self.launch_widget(account, room, command);
        }
    }

    /// Shows the permissions saved for a widget.
    pub fn manage_widget_permissions(&mut self, widget_id: &str) {
        let Some((account, room_id)) = self.widget_room() else { return };
        let command = Command::ManageWidgetPermissions { account, room_id, widget_id: widget_id.to_owned() };
        self.dispatch(command, Purpose::WidgetRun { closed: false });
    }

    /// Queues the confirmation for the destination or saved access of a widget.
    pub(super) fn widget_access(&mut self, request: RequestId, access: WidgetAccess) {
        if self.widget_running(request) {
            self.widget_contexts.insert(request, access.clone());
            self.widget_prompts.push(WidgetPrompt::Access { request, access });
            self.settle_widget_prompts();
        }
    }

    /// Continues or cancels the action whose confirmation is showing.
    pub fn confirm_widget_access(&mut self, key: PromptKey, confirmed: bool) {
        match self.take_widget_prompt(key) {
            Some(WidgetPrompt::Access { request, .. }) if confirmed => {
                self.dispatch(Command::Answer { request, answer: Answer::Continue }, Purpose::WidgetAnswer);
            }
            Some(WidgetPrompt::Access { request, .. }) => self.stop_widget_run(request),
            Some(other) => self.widget_prompts.insert(0, other),
            None => {}
        }
        self.settle_widget_prompts();
    }

    /// Opens or closes the widgets panel's menu of what can be added.
    pub fn toggle_widgets_menu(&mut self) {
        self.toggle_popover(Popover::WidgetsMenu);
    }

    /// The ways to meet by video in the room shown, the default first: a video call where the
    /// room allows one and none is under way (its Join pill joins one), joining each meeting
    /// pinned to it, and starting a meeting where the homeserver offers them.
    pub fn video_options(&self) -> Vec<VideoOption> {
        let Some(room) = self.active_room_id().and_then(|id| self.active_account()?.room(id)) else {
            return Vec::new();
        };
        let call = (room.call.via.is_some() && room.call.participants.is_empty()).then_some(VideoOption::Call);
        let joins = self
            .meetings_here()
            .into_iter()
            .map(|meeting| VideoOption::Join { widget_id: meeting.id.clone(), name: meeting.name.clone() });
        let start = room.can_start_meeting.then_some(VideoOption::Start);
        call.into_iter().chain(joins).chain(start).collect()
    }

    /// Meets by video in the room shown the way `option` says.
    pub fn choose_video_option(&mut self, option: &VideoOption) {
        self.popover = None;
        match option {
            VideoOption::Call => self.start_call(true),
            VideoOption::Join { widget_id, .. } => {
                self.run_widget(widget_id, true);
            }
            VideoOption::Start => self.start_meeting(),
        }
    }

    /// The header's video button: the room's default way to meet by video.
    pub fn start_video(&mut self) {
        if let Some(default) = self.video_options().into_iter().next() {
            self.choose_video_option(&default);
        }
    }

    /// Opens or closes the menu of the other ways to meet by video, beside the video button.
    pub fn toggle_video_options(&mut self) {
        self.toggle_popover(Popover::VideoOptions);
    }

    /// Starts a video meeting in the room shown, joined in the browser.
    pub fn start_meeting(&mut self) {
        if let Some(room_id) = self.active_room_id().map(str::to_owned) {
            self.start_meeting_in(&room_id);
        }
    }

    /// Starts a video meeting in one of the active account's rooms, joined in the browser.
    pub fn start_meeting_in(&mut self, room_id: &str) {
        self.popover = None;
        let Some(account) = self.nav.account.clone() else { return };
        let command = Command::StartMeeting { account, room_id: room_id.to_owned(), video: true, browser: true };
        self.dispatch(command, Purpose::WidgetRun { closed: false });
    }

    /// Opens the integration manager for the room shown: bots, bridges and widgets. Its
    /// request, unless no room shows.
    pub fn open_integrations(&mut self) -> Option<RequestId> {
        self.popover = None;
        let (account, room_id) = self.widget_room()?;
        let command = Command::OpenIntegrations { account: account.clone(), room_id: room_id.clone() };
        Some(self.launch_widget(account, room_id, command))
    }

    /// Whether `request` is a widget run still going that the user hasn't closed.
    fn widget_running(&self, request: RequestId) -> bool {
        matches!(self.pending.get(&request), Some(Purpose::WidgetRun { closed: false }))
    }

    /// A run's page: shown in a widget window of its own.
    pub(super) fn widget_page(&mut self, request: RequestId, page: WidgetPage) {
        if !self.widget_running(request) {
            return;
        }
        // The integration manager's page has no name of its own.
        let title = if page.title.trim().is_empty() { t!("widgets.integrations").to_owned() } else { page.title };
        self.shell.widget_windows().open(request, page.url, title, page.bridge.into());
    }

    /// A message for a run's page.
    pub(super) fn widget_message(&mut self, request: RequestId, message: String) {
        self.shell.widget_windows().post(request, message);
    }

    /// A run's page started loading a document: what it posts waits for that document.
    pub fn widget_loading(&mut self, request: RequestId) {
        if self.widget_running(request) {
            self.dispatch(Command::Answer { request, answer: Answer::WidgetLoading }, Purpose::WidgetAnswer);
        }
    }

    /// A run's page finished loading: the engine starts talking to it.
    pub fn widget_loaded(&mut self, request: RequestId) {
        if self.widget_running(request) {
            self.dispatch(Command::Answer { request, answer: Answer::WidgetLoaded }, Purpose::WidgetAnswer);
        }
    }

    /// A run's page posted `message`: the engine hears it.
    pub fn widget_posted(&mut self, request: RequestId, message: String) {
        if self.widget_running(request) {
            let answer = Answer::WidgetMessage(message);
            self.dispatch(Command::Answer { request, answer }, Purpose::WidgetAnswer);
        }
    }

    /// A widget asks to be allowed to do things: the user chooses what to grant.
    /// Asking for nothing takes back the question numbered `question`.
    pub(super) fn widget_permissions_asked(
        &mut self,
        request: RequestId,
        question: u64,
        widget: String,
        permissions: Vec<WidgetPermission>,
    ) {
        // Asking for nothing withdraws that question (the page started over): it goes unanswered.
        if permissions.is_empty() {
            self.widget_prompts.retain(|prompt| {
                !matches!(prompt, WidgetPrompt::Permissions(asked) if asked.request == request && asked.question == question)
            });
            self.settle_widget_prompts();
            return;
        }
        if !self.widget_running(request) {
            return;
        }
        let granted = vec![false; permissions.len()];
        self.widget_prompts.push(WidgetPrompt::Permissions(WidgetPermissions {
            request,
            question,
            widget,
            permissions,
            context: self.widget_contexts.get(&request).cloned(),
            granted,
        }));
        self.settle_widget_prompts();
    }

    /// The integration manager's terms, to agree to before it opens.
    pub(super) fn integration_terms(&mut self, request: RequestId, policies: Vec<Policy>) {
        if self.widget_running(request) {
            self.widget_prompts.push(WidgetPrompt::Terms { request, policies });
            self.settle_widget_prompts();
        }
    }

    /// The widget question [`Modal::WidgetPrompt`] shows, if one shows.
    pub fn widget_prompt(&self) -> Option<&WidgetPrompt> {
        let Some(Modal::WidgetPrompt(key)) = &self.modal else { return None };
        self.widget_prompts.iter().find(|prompt| prompt.key() == *key)
    }

    /// Shows the oldest widget question once no dialog shows, and closes one whose question is
    /// gone. A question waits behind any other dialog, and one that dialog replaced comes back
    /// once it closes: questions live in the queue until answered, the dialog only shows one.
    /// Whether the dialog changed. Run after every engine event, every closed dialog and every
    /// frame's poll.
    pub(super) fn settle_widget_prompts(&mut self) -> bool {
        if let Some(Modal::WidgetPrompt(key)) = &self.modal
            && !self.widget_prompts.iter().any(|prompt| prompt.key() == *key)
        {
            self.modal = None;
        }
        match (&self.modal, self.widget_prompts.first()) {
            (None, Some(next)) => {
                self.popover = None;
                self.modal = Some(Modal::WidgetPrompt(next.key()));
                true
            }
            _ => false,
        }
    }

    /// Takes the question `key` names out of the queue, closing its dialog, if it is the one
    /// showing: a press meant for a question gone since does nothing.
    fn take_widget_prompt(&mut self, key: PromptKey) -> Option<WidgetPrompt> {
        if self.modal != Some(Modal::WidgetPrompt(key)) {
            return None;
        }
        self.modal = None;
        let index = self.widget_prompts.iter().position(|prompt| prompt.key() == key)?;
        Some(self.widget_prompts.remove(index))
    }

    /// Ticks or unticks one of the permissions the question `key` asks for, if it shows.
    pub fn toggle_widget_permission(&mut self, key: PromptKey, index: usize) {
        if self.modal != Some(Modal::WidgetPrompt(key)) {
            return;
        }
        let shown = self.widget_prompts.iter_mut().find(|prompt| prompt.key() == key);
        if let Some(WidgetPrompt::Permissions(asked)) = shown
            && let Some(granted) = asked.granted.get_mut(index)
        {
            *granted = !*granted;
        }
    }

    /// Answers the permissions question `key`, if it shows: with `allow`, the ticked
    /// permissions; else none.
    pub fn answer_widget_permissions(&mut self, key: PromptKey, allow: bool) {
        match self.take_widget_prompt(key) {
            Some(WidgetPrompt::Permissions(asked)) => {
                let granted = asked
                    .permissions
                    .iter()
                    .zip(&asked.granted)
                    .filter(|(_, granted)| allow && **granted)
                    .map(|(permission, _)| permission.capability.clone())
                    .collect();
                let answer = Answer::WidgetPermissions { question: asked.question, granted };
                self.dispatch(Command::Answer { request: asked.request, answer }, Purpose::WidgetAnswer);
            }
            Some(other) => self.widget_prompts.insert(0, other),
            None => {}
        }
        self.settle_widget_prompts();
    }

    /// Agrees to the integration manager's terms `key` names, if they show: it opens.
    pub fn agree_to_integration_terms(&mut self, key: PromptKey) {
        match self.take_widget_prompt(key) {
            Some(WidgetPrompt::Terms { request, .. }) => {
                self.dispatch(Command::Answer { request, answer: Answer::Agree }, Purpose::WidgetAnswer);
            }
            Some(other) => self.widget_prompts.insert(0, other),
            None => {}
        }
        self.settle_widget_prompts();
    }

    /// Declines the integration manager's terms `key` names, if they show: it doesn't open.
    pub fn decline_integration_terms(&mut self, key: PromptKey) {
        match self.take_widget_prompt(key) {
            Some(WidgetPrompt::Terms { request, .. }) => self.stop_widget_run(request),
            Some(other) => self.widget_prompts.insert(0, other),
            None => {}
        }
        self.settle_widget_prompts();
    }

    /// Closes the widget question showing unanswered: a widget gets none of what it asked for,
    /// and the terms, not agreed to, keep the integration manager shut.
    pub(super) fn dismiss_widget_prompt(&mut self) {
        match self.widget_prompt() {
            Some(prompt @ WidgetPrompt::Permissions(_)) => self.answer_widget_permissions(prompt.key(), false),
            Some(prompt @ WidgetPrompt::Terms { .. }) => self.decline_integration_terms(prompt.key()),
            Some(prompt @ WidgetPrompt::Access { .. }) => self.confirm_widget_access(prompt.key(), false),
            None => {}
        }
    }

    /// A run's window closed: the run stops.
    pub fn widget_window_closed(&mut self, request: RequestId) {
        if self.shell.widget_windows().forget(request) {
            self.stop_widget_run(request);
        }
    }

    /// A run's window couldn't open: the run stops, and says why.
    pub fn widget_window_failed(&mut self, request: RequestId) {
        if self.shell.widget_windows().forget(request) {
            self.widget_launch_failed(request, t!("widgets.window_failed").to_owned());
            self.stop_widget_run(request);
        }
    }

    fn widget_launch_failed(&mut self, request: RequestId, error: String) {
        let launch = self.widget_launches.remove(&request);
        self.remember_widget_launch_error(launch, error);
    }

    pub(super) fn open_widget_url(&mut self, request: RequestId, url: &str, tx: &mut rune::Tx) {
        let launch = self.widget_launches.get(&request).cloned();
        if tx
            .open_url_then(url, move |state: &mut Self, result, _| {
                if result.is_err() {
                    state.remember_widget_launch_error(launch, t!("widgets.browser_failed").to_owned());
                }
            })
            .is_err()
        {
            self.widget_launch_failed(request, t!("widgets.browser_failed").to_owned());
        }
    }

    fn remember_widget_launch_error(&mut self, launch: Option<(AccountId, String, Command)>, error: String) {
        if let Some((account, room, command)) = launch
            && let Some(data) = self.accounts.get_mut(&account)
        {
            data.widget_launch_errors.insert(room, (error.clone(), command));
        }
        self.notice = Some(super::Notice::problem(tf!("notice.widget_run", error = error)));
    }

    /// Stops a run the user is done with; its cancelled finish says nothing.
    fn stop_widget_run(&mut self, request: RequestId) {
        if let Some(Purpose::WidgetRun { closed }) = self.pending.get_mut(&request)
            && !*closed
        {
            *closed = true;
            self.dispatch(Command::Cancel(request), Purpose::Cancel);
        }
        self.drop_widget_dialogs(request);
    }

    /// A run ended: its window and dialogs close, and a failure the user didn't cause shows.
    pub(super) fn widget_run_finished(&mut self, request: RequestId, closed: bool, error: Option<String>) {
        self.shell.widget_windows().close(request);
        self.drop_widget_dialogs(request);
        if let (false, Some(error)) = (closed, error)
            && error != matrix_core::Error::Cancelled.to_string()
        {
            self.widget_launch_failed(request, error);
        }
        self.widget_launches.remove(&request);
    }

    /// What was asked of the widget windows, headless.
    #[cfg(test)]
    pub fn widget_window_calls(&self) -> &[crate::platform::widget_window::WindowCall] {
        self.shell.widget_window_calls()
    }

    /// Closes and forgets the questions a run asked; the next one queued shows.
    fn drop_widget_dialogs(&mut self, request: RequestId) {
        self.widget_contexts.remove(&request);
        self.widget_prompts.retain(|prompt| prompt.request() != request);
        self.settle_widget_prompts();
    }
}

/// A way to meet by video in a room ([`State::video_options`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoOption {
    /// A video call (MatrixRTC).
    Call,
    /// Joining a meeting pinned to the room, in the browser: its widget and name.
    Join { widget_id: String, name: String },
    /// Starting a meeting, joined in the browser.
    Start,
}

/// A question a running widget waits on the user for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WidgetPrompt {
    /// What it asks to be allowed to do.
    Permissions(WidgetPermissions),
    /// The widget run is waiting for confirmation to proceed.
    Access { request: RequestId, access: WidgetAccess },
    /// The integration manager's terms, to agree to before it opens.
    Terms { request: RequestId, policies: Vec<Policy> },
}

impl WidgetPrompt {
    /// The run that asks.
    pub fn request(&self) -> RequestId {
        self.key().request
    }

    /// Which question this is, for the dialog that shows it and the presses it takes.
    pub fn key(&self) -> PromptKey {
        match self {
            Self::Permissions(asked) => asked.key(),
            Self::Access { request, .. } => PromptKey { request: *request, question: Some(0) },
            Self::Terms { request, .. } => PromptKey { request: *request, question: None },
        }
    }
}

/// Which widget question a dialog shows: its run, and the question's number within the run
/// (none for the integration manager's terms, one per run). The dialog's controls carry it in
/// their keys and presses, so a press can only answer the question it was shown for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PromptKey {
    pub request: RequestId,
    pub question: Option<u64>,
}

impl PromptKey {
    /// A stable element key part for it: `<request>-<question>` (`<request>-terms`).
    pub fn id(self) -> String {
        match self.question {
            Some(question) => format!("{}-{question}", self.request.0),
            None => format!("{}-terms", self.request.0),
        }
    }
}

#[cfg(test)]
mod tests;
