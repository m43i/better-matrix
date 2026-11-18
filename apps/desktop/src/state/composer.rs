//! The message composer: rich text, pending attachments and link entry. Turns into
//! [`Outgoing`] messages on send.

use std::path::PathBuf;

use matrix_core::{AccountId, Outgoing, TimelineKey, Upload};
use rune::{Block, Mark, TextEdit};

/// Every formatting action the composer offers. The toolbar renders this list; shortcuts and
/// menus can call [`Composer::format`] with the same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    BulletList,
    NumberedList,
    Quote,
    CodeBlock,
}

impl Format {
    pub const ALL: [Format; 9] = [
        Format::Bold,
        Format::Italic,
        Format::Strike,
        Format::Code,
        Format::Link,
        Format::BulletList,
        Format::NumberedList,
        Format::Quote,
        Format::CodeBlock,
    ];

    fn mark(self) -> Option<Mark> {
        match self {
            Format::Bold => Some(Mark::Bold),
            Format::Italic => Some(Mark::Italic),
            Format::Strike => Some(Mark::Strike),
            Format::Code => Some(Mark::Code),
            _ => None,
        }
    }

    fn block(self) -> Option<Block> {
        match self {
            Format::BulletList => Some(Block::Bullet(0)),
            Format::NumberedList => Some(Block::Numbered(0)),
            Format::Quote => Some(Block::Quote),
            Format::CodeBlock => Some(Block::Code),
            _ => None,
        }
    }

    /// A stable name for keys and tests, the same in every language.
    pub fn id(self) -> &'static str {
        match self {
            Format::Bold => "bold",
            Format::Italic => "italic",
            Format::Strike => "strike",
            Format::Code => "code",
            Format::Link => "link",
            Format::BulletList => "bullets",
            Format::NumberedList => "numbers",
            Format::Quote => "quote",
            Format::CodeBlock => "code-block",
        }
    }

    /// Whether this starts a new group in the toolbar.
    pub fn starts_group(self) -> bool {
        matches!(self, Format::Link | Format::BulletList | Format::CodeBlock)
    }
}

/// A voice memo being recorded, and the timeline it is for: once that leaves the screen, the
/// memo is dropped.
#[derive(Clone)]
pub struct VoiceMemo {
    pub recorder: crate::platform::sound::Recorder,
    pub timeline: (matrix_core::AccountId, matrix_core::TimelineKey),
}

impl std::fmt::Debug for VoiceMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceMemo").field("seconds", &self.recorder.elapsed()).finish()
    }
}

/// The message a draft answers, shown above the field while replying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyTarget {
    pub event_id: String,
    pub sender_name: String,
    pub excerpt: String,
}

#[derive(Debug)]
pub struct Composer {
    pub edit: TextEdit,
    pub attachments: Vec<Upload>,
    /// The formatting toolbar is open (the Aa button).
    pub formatting: bool,
    /// A mark just applied to a selection, and where the selection ended: text typed on from
    /// there stays plain, so formatting a word doesn't format what follows it.
    fresh: Option<(Mark, usize)>,
    /// A voice memo being recorded, in place of the field until it is sent or dropped.
    pub voice: Option<VoiceMemo>,
    /// The URL being typed for a link, while link entry is open.
    pub link: Option<TextEdit>,
    /// The message this draft answers, when replying.
    pub reply_to: Option<ReplyTarget>,
    /// The user's own message being edited. The field holds its text, and sending changes it
    /// rather than sending a new message.
    pub editing: Option<Editing>,
    /// The selection and its content-space caret line last revealed by the editor.
    pub revealed_caret: Option<(usize, usize, f32)>,
    /// The timeline whose draft this composer holds; `State::follow_drafts` swaps drafts as
    /// the timeline on screen changes.
    pub owner: Option<(AccountId, TimelineKey)>,
    /// [`Composer::stored_draft`], and a fingerprint of the field it was read from: drafts are
    /// checked every frame, and read again only once the field changed.
    stored: Option<(u64, Option<matrix_core::Draft>)>,
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            edit: new_edit(),
            attachments: Vec::new(),
            formatting: false,
            fresh: None,
            voice: None,
            link: None,
            reply_to: None,
            editing: None,
            revealed_caret: None,
            owner: None,
            stored: None,
        }
    }
}

/// A field's text as Matrix HTML: only when it carries formatting.
fn html_of(edit: &TextEdit) -> Option<String> {
    let formatted = !edit.marks().is_empty() || edit.blocks().iter().any(|b| *b != Block::Paragraph);
    formatted.then(|| edit.to_html())
}

/// Which message an edit changes: the account's message `event_id` in the timeline `key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTarget {
    pub account: AccountId,
    pub key: TimelineKey,
    pub event_id: String,
}

impl EditTarget {
    /// Whether both edit the same message, from whichever timeline (a thread's root shows in
    /// the room too).
    pub fn same_message(&self, other: &EditTarget) -> bool {
        self.account == other.account && self.key.room_id == other.key.room_id && self.event_id == other.event_id
    }
}

/// One of the user's messages being edited in a composer.
#[derive(Debug)]
pub struct Editing {
    pub target: EditTarget,
    /// The message as it read, text and HTML: an unchanged edit isn't sent.
    original: (String, Option<String>),
    /// The draft set aside while editing.
    draft: Box<Draft>,
}

/// A message being written: the field, the message it answers and its attachments. It is set
/// aside while an edit takes the field, and kept per timeline while another one shows.
#[derive(Debug)]
pub struct Draft {
    edit: TextEdit,
    reply_to: Option<ReplyTarget>,
    attachments: Vec<Upload>,
}

impl Draft {
    /// Nothing written, answered or attached.
    pub fn is_empty(&self) -> bool {
        self.edit.text.trim().is_empty() && self.reply_to.is_none() && self.attachments.is_empty()
    }

    /// What of it this device's store keeps: the text and its formatting, unless blank.
    pub fn stored(&self) -> Option<matrix_core::Draft> {
        stored_draft(&self.edit)
    }

    /// Takes a draft read from the store as its text, the caret at its end.
    pub fn restore(&mut self, draft: &matrix_core::Draft) {
        self.edit = field_of(draft);
    }
}

/// A field holding a draft read from the store, the caret at its end.
fn field_of(draft: &matrix_core::Draft) -> TextEdit {
    let mut field = new_edit();
    match &draft.html {
        Some(html) => {
            let _ = field.paste_html(html, &draft.body);
        }
        None => field.insert(&draft.body),
    }
    field.clear_history();
    field
}

/// A field's text and formatting as the store keeps a draft, unless blank.
fn stored_draft(edit: &TextEdit) -> Option<matrix_core::Draft> {
    (!edit.text.trim().is_empty()).then(|| matrix_core::Draft { body: edit.text.clone(), html: html_of(edit) })
}

/// An edit on its way: the message, its new text and formatting, and the field as the user left
/// it, which comes back if the edit fails.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub target: EditTarget,
    pub body: String,
    pub html: Option<String>,
    field: TextEdit,
    original: (String, Option<String>),
}

/// A rich field limited to what Matrix HTML carries (no underline).
fn new_edit() -> TextEdit {
    TextEdit::new("")
        .rich()
        .allow_marks([Mark::Bold, Mark::Italic, Mark::Strike, Mark::Code, Mark::Link(String::new())])
        .allow_blocks([Block::Bullet(0), Block::Numbered(0), Block::Quote, Block::Code])
}

impl Composer {
    pub fn is_active(&self, format: Format) -> bool {
        match (format.mark(), format.block()) {
            (Some(mark), _) => self.edit.has_mark(&mark),
            (_, Some(block)) => std::mem::discriminant(&self.edit.active_block()) == std::mem::discriminant(&block),
            _ => self.edit.active_link().is_some(),
        }
    }

    pub fn format(&mut self, format: Format) {
        match (format.mark(), format.block()) {
            (Some(mark), _) => {
                let selection = (self.edit.anchor.min(self.edit.focus), self.edit.anchor.max(self.edit.focus));
                self.edit.toggle_mark(mark.clone());
                self.fresh = (selection.0 < selection.1 && self.edit.has_mark(&mark)).then_some((mark, selection.1));
            }
            (_, Some(block)) => self.edit.toggle_block(block),
            _ if self.edit.active_link().is_some() => self.edit.set_link(None),
            _ => self.link = Some(TextEdit::new("https://")),
        }
    }

    /// Applies the typed URL to the selection and closes link entry.
    pub fn confirm_link(&mut self) {
        if let Some(url) = self.link.take().map(|edit| edit.text.trim().to_owned())
            && !url.is_empty()
            && url != "https://"
        {
            self.edit.set_link(Some(&url));
        }
    }

    /// Pastes a web address as a link: over a selection, the selection links to it; else the
    /// address goes in as a link, and what is typed after it stays plain. One undo step.
    pub fn paste_link(&mut self, url: &str) {
        self.edit.undo_group(|edit| {
            let (start, end) = (edit.anchor.min(edit.focus), edit.anchor.max(edit.focus));
            let end = if start == end {
                edit.insert(url);
                start + url.len()
            } else {
                end
            };
            edit.set_selection(start, end);
            edit.set_link(Some(url));
            edit.set_selection(end, end);
            // An empty selection arms the style typed next: without the link.
            edit.set_link(None);
        });
    }

    /// Starts editing the user's message `event_id` in `key`: the field takes its text and
    /// formatting, and the draft (with its reply and attachments) is set aside until the edit
    /// is sent or dropped.
    pub fn start_edit(&mut self, target: EditTarget, body: &str, html: Option<&str>) {
        let mut field = new_edit();
        match html {
            Some(html) => {
                let _ = field.paste_html(html, body);
            }
            None => field.insert(body),
        }
        field.clear_history();
        let original = (field.text.trim().to_owned(), html_of(&field));
        self.edit_with(target, field, original);
    }

    fn edit_with(&mut self, target: EditTarget, mut field: TextEdit, original: (String, Option<String>)) {
        self.cancel_edit();
        field.focused = true;
        let draft = Draft {
            edit: std::mem::replace(&mut self.edit, field),
            reply_to: self.reply_to.take(),
            attachments: std::mem::take(&mut self.attachments),
        };
        self.link = None;
        self.editing = Some(Editing { target, original, draft: Box::new(draft) });
    }

    /// Stops editing: the draft set aside comes back.
    pub fn cancel_edit(&mut self) {
        let Some(editing) = self.editing.take() else { return };
        let Draft { edit, reply_to, attachments } = *editing.draft;
        self.edit = edit;
        self.reply_to = reply_to;
        self.attachments = attachments;
        self.link = None;
    }

    /// The edit under way, as the user left it, taken out of the field: the draft comes back.
    pub fn set_aside_edit(&mut self) -> Option<Edit> {
        let editing = self.editing.as_ref()?;
        let edit = Edit {
            target: editing.target.clone(),
            body: self.edit.text.trim().to_owned(),
            html: html_of(&self.edit),
            field: self.edit.clone(),
            original: editing.original.clone(),
        };
        self.cancel_edit();
        Some(edit)
    }

    /// The edit to send, if one is under way and changed the message; the draft comes back
    /// either way. An emptied message isn't sent: it stays as it was.
    pub fn take_edit(&mut self) -> Option<Edit> {
        self.set_aside_edit()
            .filter(|edit| !edit.body.is_empty() && (&edit.body, &edit.html) != (&edit.original.0, &edit.original.1))
    }

    /// Takes an edit that failed back into the field, as the user left it, to try again.
    pub fn resume_edit(&mut self, edit: Edit) {
        self.edit_with(edit.target, edit.field, edit.original);
    }

    /// The draft written here, taken out: the composer is left empty, its field as focused as
    /// it was. Call with no edit under way (`set_aside_edit` first).
    pub fn take_draft(&mut self) -> Draft {
        let mut field = new_edit();
        field.focused = self.edit.focused;
        self.link = None;
        self.fresh = None;
        self.revealed_caret = None;
        Draft {
            edit: std::mem::replace(&mut self.edit, field),
            reply_to: self.reply_to.take(),
            attachments: std::mem::take(&mut self.attachments),
        }
    }

    /// Takes back a draft written here before, keeping the field as focused as it is.
    pub fn put_draft(&mut self, draft: Draft) {
        let focused = self.edit.focused;
        let Draft { edit, reply_to, attachments } = draft;
        self.edit = edit;
        self.edit.focused = focused;
        self.reply_to = reply_to;
        self.attachments = attachments;
        self.link = None;
        self.fresh = None;
        self.revealed_caret = None;
    }

    /// The draft's text as the store keeps it: the draft set aside while an edit holds the field.
    pub fn stored_draft(&self) -> Option<matrix_core::Draft> {
        match &self.editing {
            Some(editing) => editing.draft.stored(),
            None => stored_draft(&self.edit),
        }
    }

    /// Reads [`Composer::stored_draft`] again if the field it comes from changed since the last
    /// call, for [`Composer::stored_draft_read`]: cheap enough to call every frame.
    pub fn read_stored_draft(&mut self) {
        use std::hash::{Hash, Hasher};
        let source = match &self.editing {
            Some(editing) => &editing.draft.edit,
            None => &self.edit,
        };
        // A blank field keeps no draft, whatever else it holds: nothing to read.
        if source.text.trim().is_empty() {
            self.stored = None;
            return;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.editing.is_some(), source).hash(&mut hasher);
        let fingerprint = hasher.finish();
        if self.stored.as_ref().is_none_or(|(known, _)| *known != fingerprint) {
            self.stored = Some((fingerprint, self.stored_draft()));
        }
    }

    /// [`Composer::stored_draft`] as [`Composer::read_stored_draft`] last read it.
    pub fn stored_draft_read(&self) -> Option<&matrix_core::Draft> {
        self.stored.as_ref().and_then(|(_, draft)| draft.as_ref())
    }

    /// Puts a draft read from the store into the draft's text, the caret at its end: the field,
    /// or the draft set aside under an edit. A reply and attachments stay.
    pub fn restore_draft(&mut self, draft: &matrix_core::Draft) {
        match &mut self.editing {
            Some(editing) => editing.draft.restore(draft),
            None => {
                let focused = self.edit.focused;
                self.edit = field_of(draft);
                self.edit.focused = focused;
            }
        }
    }

    /// Before a key reaches the field: typing straight on from a freshly formatted selection
    /// starts plain. The mark keeps covering the selection itself.
    pub fn before_key(&mut self, key: &rune::KeyEvent) {
        if !matches!(key.key, rune::Key::Text(_)) {
            return;
        }
        if let Some((mark, end)) = self.fresh.take()
            && self.edit.anchor == end
            && self.edit.focus == end
            && self.edit.has_mark(&mark)
        {
            self.edit.toggle_mark(mark);
        }
    }

    /// Opens or closes the formatting toolbar.
    pub fn toggle_formatting(&mut self) {
        self.formatting = !self.formatting;
    }

    pub fn cancel_link(&mut self) {
        self.link = None;
    }

    pub fn attach(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for upload in paths.into_iter().map(Upload::Path) {
            if !self.attachments.contains(&upload) {
                self.attachments.push(upload);
            }
        }
    }

    /// Attaches a pasted image, named `image.png`, `image-2.png` and so on within this draft.
    pub fn attach_pasted_image(&mut self, png: &[u8]) {
        let taken = |name: &str| self.attachments.iter().any(|upload| upload.name() == name);
        let name = (1..)
            .map(|n| if n == 1 { "image.png".to_owned() } else { format!("image-{n}.png") })
            .find(|name| !taken(name))
            .unwrap_or_default();
        self.attachments.push(Upload::Bytes { name, bytes: png.into() });
    }

    pub fn remove_attachment(&mut self, index: usize) {
        if index < self.attachments.len() {
            self.attachments.remove(index);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.edit.text.trim().is_empty() && self.attachments.is_empty()
    }

    /// Whether Enter should send rather than let the editor continue a list or code block.
    pub fn enter_sends(&self) -> bool {
        matches!(self.edit.active_block(), Block::Paragraph | Block::Quote)
    }

    /// Everything to send, text first, and a fresh composer. HTML is sent only when the text
    /// carries formatting. The reply target, if any, goes with the first message.
    pub fn take_outgoing(&mut self) -> (Option<ReplyTarget>, Vec<Outgoing>) {
        let mut outgoing = Vec::new();
        let body = self.edit.text.trim().to_owned();
        if !body.is_empty() {
            outgoing.push(Outgoing::Text { body, html: html_of(&self.edit) });
        }
        outgoing.extend(self.attachments.drain(..).map(Outgoing::File));
        // The emptied field keeps its focus, so the next message can follow straight away.
        let focused = self.edit.focused;
        self.edit = new_edit();
        self.edit.focused = focused;
        self.link = None;
        (self.reply_to.take(), outgoing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composer(text: &str) -> Composer {
        let mut c = Composer::default();
        c.edit.insert(text);
        c
    }

    #[test]
    fn plain_text_should_be_sent_without_html() {
        assert_eq!(composer("hello").take_outgoing().1, [Outgoing::Text { body: "hello".into(), html: None }]);
    }

    #[test]
    fn bold_text_should_be_sent_with_html() {
        let mut c = composer("hello");
        c.edit.set_selection(0, 5);
        c.format(Format::Bold);
        let Some(Outgoing::Text { html: Some(html), .. }) = c.take_outgoing().1.into_iter().next() else {
            panic!("expected formatted text");
        };
        assert!(html.contains("<b>hello</b>") || html.contains("<strong>hello</strong>"), "{html}");
    }

    #[test]
    fn a_mark_toggled_twice_without_a_selection_should_be_off_again() {
        let mut c = Composer::default();
        c.format(Format::Bold);
        c.format(Format::Bold);
        c.edit.insert("plain");
        assert!(c.edit.marks().is_empty());
    }

    #[test]
    fn a_mark_toggled_without_a_selection_should_apply_to_what_is_typed() {
        let mut c = Composer::default();
        c.format(Format::Bold);
        c.edit.insert("loud");
        assert!(c.is_active(Format::Bold));
        assert!(!c.edit.marks().is_empty());
    }

    #[test]
    fn formatting_a_selection_should_not_format_what_is_typed_after_it() {
        let mut c = Composer::default();
        c.edit.insert("word");
        c.edit.anchor = 0;
        c.edit.focus = 4;
        c.format(Format::Bold);
        c.edit.anchor = 4;
        c.edit.focus = 4;
        c.before_key(&rune::KeyEvent { key: rune::Key::Text("x".into()), mods: rune::Mods::default() });
        assert!(!c.is_active(Format::Bold));
    }

    #[test]
    fn formatting_a_selection_should_mark_exactly_it() {
        let mut c = Composer::default();
        c.edit.insert("one two");
        c.edit.anchor = 4;
        c.edit.focus = 7;
        c.format(Format::Bold);
        assert_eq!(c.edit.to_html(), "<p>one <strong>two</strong></p>");
    }

    #[test]
    fn pasted_images_should_get_their_own_names() {
        let mut c = Composer::default();
        c.attach_pasted_image(b"one");
        c.attach_pasted_image(b"two");
        let names: Vec<_> = c.attachments.iter().map(Upload::name).collect();
        assert_eq!(names, ["image.png", "image-2.png"]);
    }

    #[test]
    fn attachments_should_be_sent_after_the_text() {
        let mut c = composer("look");
        c.attach([PathBuf::from("/tmp/a.png")]);
        assert_eq!(c.take_outgoing().1[1], Outgoing::File(Upload::Path(PathBuf::from("/tmp/a.png"))));
    }

    #[test]
    fn attaching_the_same_file_twice_should_keep_one() {
        let mut c = Composer::default();
        c.attach([PathBuf::from("/a"), PathBuf::from("/a")]);
        assert_eq!(c.attachments.len(), 1);
    }

    #[test]
    fn sending_should_leave_an_empty_composer() {
        let mut c = composer("hi");
        c.take_outgoing();
        assert!(c.is_empty());
    }

    #[test]
    fn sending_should_keep_the_field_focused() {
        let mut c = composer("hi");
        c.edit.focused = true;
        c.take_outgoing();
        assert!(c.edit.focused);
    }

    #[test]
    fn enter_should_continue_a_list_instead_of_sending() {
        let mut c = composer("item");
        c.format(Format::BulletList);
        assert!(!c.enter_sends());
    }

    #[test]
    fn a_pasted_address_should_go_in_as_a_link() {
        let mut c = composer("see ");
        c.paste_link("https://matrix.org");
        assert_eq!(c.edit.to_html(), "<p>see <a href=\"https://matrix.org\">https://matrix.org</a></p>");
    }

    #[test]
    fn typing_after_a_pasted_link_should_be_plain() {
        let mut c = Composer::default();
        c.paste_link("https://matrix.org");
        c.edit.insert(" now");
        assert_eq!(c.edit.to_html(), "<p><a href=\"https://matrix.org\">https://matrix.org</a> now</p>");
    }

    #[test]
    fn an_address_pasted_over_a_selection_should_link_it() {
        let mut c = composer("the blog");
        c.edit.set_selection(4, 8);
        c.paste_link("https://matrix.org/blog");
        assert_eq!(c.edit.to_html(), "<p>the <a href=\"https://matrix.org/blog\">blog</a></p>");
    }

    fn target() -> EditTarget {
        EditTarget { account: AccountId::new("@a:x"), key: TimelineKey::room("!r:x"), event_id: "$1".into() }
    }

    #[test]
    fn editing_should_put_the_message_in_the_field_and_send_it_back_changed() {
        let mut c = Composer::default();
        c.start_edit(target(), "helo", None);
        c.edit.insert("!");
        let edit = c.take_edit().map(|edit| (edit.target, edit.body, edit.html));
        assert_eq!(edit, Some((target(), "helo!".to_owned(), None)));
        assert!(c.editing.is_none() && c.is_empty());
    }

    #[test]
    fn editing_a_formatted_message_should_keep_its_formatting() {
        let mut c = Composer::default();
        c.start_edit(target(), "loud", Some("<strong>loud</strong>"));
        c.edit.insert("er");
        let html = c.take_edit().and_then(|edit| edit.html).unwrap_or_default();
        assert!(html.contains("<strong>louder</strong>"), "{html}");
    }

    #[test]
    fn an_emptied_edit_should_not_be_sent() {
        let mut c = Composer::default();
        c.start_edit(target(), "x", None);
        c.edit.set_selection(0, 1);
        c.edit.insert("");
        assert!(c.take_edit().is_none());
    }

    #[test]
    fn an_unchanged_edit_should_not_be_sent() {
        let mut c = Composer::default();
        c.start_edit(target(), "same", None);
        assert!(c.take_edit().is_none());
    }

    #[test]
    fn a_draft_should_come_back_after_editing() {
        let mut c = composer("half a thought");
        c.attach([PathBuf::from("/a.png")]);
        c.start_edit(target(), "older", None);
        assert!(c.attachments.is_empty());
        c.cancel_edit();
        assert_eq!((c.edit.text.as_str(), c.attachments.len()), ("half a thought", 1));
    }

    #[test]
    fn a_failed_edit_should_come_back_as_the_user_left_it() {
        let mut c = composer("draft");
        c.start_edit(target(), "helo", None);
        c.edit.insert("!");
        let Some(edit) = c.take_edit() else { panic!("expected an edit") };
        c.resume_edit(edit);
        assert_eq!(c.edit.text, "helo!");
        c.cancel_edit();
        assert_eq!(c.edit.text, "draft");
    }

    #[test]
    fn the_link_button_should_open_link_entry() {
        let mut c = composer("site");
        c.format(Format::Link);
        assert!(c.link.is_some());
    }
}
