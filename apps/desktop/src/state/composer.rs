//! The message composer: rich text, pending attachments and link entry. Turns into
//! [`Outgoing`] messages on send.

use std::path::PathBuf;

use matrix_core::{Outgoing, Upload};
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

/// A voice memo being recorded, the water its voice stirs, and the timeline it is for: once
/// that leaves the screen, the memo is dropped.
#[derive(Clone)]
pub struct VoiceMemo(
    pub crate::platform::sound::Recorder,
    pub super::water::Water,
    pub (matrix_core::AccountId, matrix_core::TimelineKey),
);

impl std::fmt::Debug for VoiceMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceMemo").field("seconds", &self.0.elapsed()).finish()
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
    /// The user's own message being edited: its event ID. The field holds its text, and sending
    /// changes it rather than sending a new message.
    pub editing: Option<String>,
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
        }
    }
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

    /// Starts editing the user's message `event_id`: the field takes its text and formatting,
    /// in place of the draft.
    pub fn start_edit(&mut self, event_id: &str, body: &str, html: Option<&str>) {
        let mut edit = new_edit();
        match html {
            Some(html) => {
                let _ = edit.paste_html(html, body);
            }
            None => edit.insert(body),
        }
        edit.clear_history();
        edit.focused = true;
        self.edit = edit;
        self.editing = Some(event_id.to_owned());
        self.reply_to = None;
        self.link = None;
    }

    /// Stops editing, leaving an empty field.
    pub fn cancel_edit(&mut self) {
        if self.editing.take().is_some() {
            self.edit = new_edit();
            self.link = None;
        }
    }

    /// The edit to send, if one is under way: the message's event ID, its new text and its
    /// formatting. Leaves an empty field. An emptied message isn't sent: it stays as it was.
    pub fn take_edit(&mut self) -> Option<(String, String, Option<String>)> {
        let event_id = self.editing.take()?;
        let (body, html) = (self.edit.text.trim().to_owned(), self.html());
        self.edit = new_edit();
        self.link = None;
        (!body.is_empty()).then_some((event_id, body, html))
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

    /// The draft as Matrix HTML: only when it carries formatting.
    fn html(&self) -> Option<String> {
        let formatted = !self.edit.marks().is_empty() || self.edit.blocks().iter().any(|b| *b != Block::Paragraph);
        formatted.then(|| self.edit.to_html())
    }

    /// Everything to send, text first, and a fresh composer. HTML is sent only when the text
    /// carries formatting. The reply target, if any, goes with the first message.
    pub fn take_outgoing(&mut self) -> (Option<ReplyTarget>, Vec<Outgoing>) {
        let mut outgoing = Vec::new();
        let body = self.edit.text.trim().to_owned();
        if !body.is_empty() {
            outgoing.push(Outgoing::Text { body, html: self.html() });
        }
        outgoing.extend(self.attachments.drain(..).map(Outgoing::File));
        self.edit = new_edit();
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

    #[test]
    fn editing_should_put_the_message_in_the_field_and_send_it_back_changed() {
        let mut c = Composer::default();
        c.start_edit("$1", "helo", None);
        c.edit.insert("!");
        assert_eq!(c.take_edit(), Some(("$1".to_owned(), "helo!".to_owned(), None)));
        assert!(c.editing.is_none() && c.is_empty());
    }

    #[test]
    fn editing_a_formatted_message_should_keep_its_formatting() {
        let mut c = Composer::default();
        c.start_edit("$1", "loud", Some("<strong>loud</strong>"));
        let Some((_, _, Some(html))) = c.take_edit() else { panic!("expected formatted text") };
        assert!(html.contains("<strong>loud</strong>"), "{html}");
    }

    #[test]
    fn an_emptied_edit_should_not_be_sent() {
        let mut c = Composer::default();
        c.start_edit("$1", "x", None);
        c.edit.set_selection(0, 1);
        c.edit.insert("");
        assert_eq!(c.take_edit(), None);
    }

    #[test]
    fn the_link_button_should_open_link_entry() {
        let mut c = composer("site");
        c.format(Format::Link);
        assert!(c.link.is_some());
    }
}
