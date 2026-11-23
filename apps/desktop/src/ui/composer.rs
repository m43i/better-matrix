//! The composer, for the room and for the open thread, laid out like Slack's: a "replying to"
//! bar, the formatting toolbar while Aa is on, the rich text field, pending attachments, and a
//! bottom bar with attach, emoji, Aa and a voice memo, and send at the right. Presses on the
//! bars leave the field focused, so formatting and emoji go where the caret is. Keys carry the
//! composer's prefix, so both can be on screen at once.

use rune::{Color, Cx, El, Key, auto_grow, col, el, icon, row, text, text_input};
use rune_kit::{ScrollBar, ScrollBarStyle};

use super::components::icon_button;
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{ComposerTarget, EmojiFor, Format, Popover, State, composer_field};

const EDITOR_LINES: usize = 8;
const EDITOR_TOP: f32 = 14.0;
const EDITOR_BOTTOM: f32 = 8.0;

#[cfg(test)]
mod tests;

/// A key unique to this composer.
fn key(target: ComposerTarget, name: &str) -> String {
    match target {
        ComposerTarget::Room => name.to_owned(),
        ComposerTarget::Thread => format!("thread-{name}"),
    }
}

pub fn composer(s: &State, target: ComposerTarget, placeholder: &str, p: &Palette, cx: &mut Cx<State>) -> El {
    let c = s.composer(target);
    let pill_palette = *p;
    let input = text_input(&c.edit, font::BODY)
        .key(composer_field(target))
        .placeholder(placeholder.to_owned(), p.text_muted)
        .color(p.text)
        .w_full()
        .min_h(24.0)
        .on_edit(move |s: &mut State| &mut s.composer_mut(target).edit)
        .on_submit(move |s: &mut State, tx| s.send_message(target, tx))
        // A pasted image or file joins the draft as an attachment; text pastes as usual.
        .on_input_paste(move |s: &mut State, content, _| s.paste_into_composer(target, content))
        .pill_style(move |id| pill_look(id, &pill_palette))
        .on_input_key(move |s: &mut State, key, tx| {
            // A list of suggestions takes the keys that move through it, pick or dismiss it.
            if s.suggestion_key(target, key) {
                return true;
            }
            // A key that would join a spoiler or mention onto a code block does nothing.
            if s.composer(target).would_strip_pills(key) {
                return true;
            }
            s.composer_mut(target).before_key(key);
            let c = s.composer(target);
            match key.key {
                // Inside a Markdown code fence, Enter goes on to the next line of code.
                Key::Enter if !key.mods.shift && c.in_code_fence() => {
                    s.composer_mut(target).edit.insert("\n");
                    true
                }
                Key::Enter if !key.mods.shift && c.enter_sends() => {
                    s.send_message(target, tx);
                    true
                }
                // Up in an empty field edits the user's last message, as in other chat apps.
                Key::Up if c.is_empty() && c.editing.is_none() => s.edit_last(target, tx),
                Key::Escape if c.editing.is_some() => {
                    s.cancel_edit(target);
                    true
                }
                Key::Escape if c.reply_to.is_some() => {
                    s.cancel_reply(target);
                    true
                }
                _ => false,
            }
        });
    let input = col()
        .key(key(target, "composer-editor"))
        .w_full()
        .pt(EDITOR_TOP)
        .pb(EDITOR_BOTTOM)
        .preserve_focus_on_press(true)
        .cursor(rune::Cursor::Text)
        .on_press_tx(move |_: &mut State, tx| tx.focus(composer_field(target), false))
        .child(
            auto_grow(input, 1, EDITOR_LINES)
                .key(key(target, "composer-scroll"))
                .px(16.0)
                .children(selection_caret(s, target, cx))
                .scroll_bar_styled(
                    ScrollBarStyle::neutral(s.dark(cx.system_dark())).thumb(p.divider).thumb_hover(p.text_muted),
                ),
        );

    let mut frame = col()
        .key(key(target, "composer-frame"))
        .w_full()
        .radius(10.0)
        .border(1.0, if c.edit.focused { p.text_muted } else { p.divider })
        .fill(p.surface);
    if let Some(reply) = &c.reply_to {
        frame = frame.child(reply_bar(target, &reply.sender_name, &reply.excerpt, p));
    }
    if c.editing.is_some() {
        frame = frame.child(edit_bar(target, p));
    }
    if c.formatting {
        frame = frame.child(toolbar(s, target, p));
    }
    if let Some(link) = &c.link {
        frame = frame.child(link_entry(target, link, p));
    }
    frame = match &c.voice {
        Some(memo) => frame.child(recording(target, memo, p)),
        None => frame.child(input),
    };
    if !c.attachments.is_empty() {
        frame = frame.child(attachments(s, target, p));
    }
    // While a memo records, its row has everything that applies (discard, send); the bar's
    // tools are for writing.
    if c.voice.is_none() {
        frame = frame.child(bottom_bar(s, target, p));
    }
    // The room's composer keeps a line under it for who is typing; the thread's, its margin.
    let below = match target {
        ComposerTarget::Room => typing_line(s, p),
        ComposerTarget::Thread => el().h(TYPING_LINE),
    };
    // While a memo records, the voice lights a glow around the frame, drawn under it.
    let glow = c.voice.as_ref().map(|memo| voice_glow(target, memo, s.reduced_motion.get(), p));
    let framed = col().key(key(target, "composer-framed")).w_full().children(glow).child(frame);
    col().w_full().px(20.0).pt(4.0).children(scheduled_bar(s, target, p)).child(framed).child(below)
}

/// Rune follows keyboard edits and a selection dragged past the editor's edge itself; the
/// caret in a code fence needs revealing here, since Enter's new line there is the composer's
/// own. A selection is Rune's to follow, so only a caret asks. The marker lives in content
/// coordinates, so a wheel scroll alone never asks to reveal it.
fn selection_caret(s: &State, target: ComposerTarget, cx: &Cx<State>) -> Option<El> {
    let c = s.composer(target);
    let edit = &c.edit;
    if edit.anchor != edit.focus || !c.in_code_fence() {
        return None;
    }
    let caret = cx.caret(composer_field(target))?;
    let field = cx.window_layout(composer_field(target))?;
    let viewport = cx.window_layout(&key(target, "composer-scroll"))?;
    let outside = caret[1] < viewport[1] || caret[1] + caret[3] > viewport[1] + viewport[3];
    let top = caret[1] - field[1];
    let selection = (edit.anchor, edit.focus);
    let marker = key(target, "composer-caret");
    Some(el().key(marker.clone()).absolute(0.0, top).size(1.0, caret[3]).a11y_hidden().on_layout_tx(
        move |s: &mut State, _, tx| {
            let c = s.composer_mut(target);
            if (c.edit.anchor, c.edit.focus) != selection
                || c.revealed_caret
                    .is_some_and(|(anchor, focus, y)| (anchor, focus) == selection && (y - top).abs() < 0.5)
            {
                tx.live_only();
                return;
            }
            c.revealed_caret = Some((selection.0, selection.1, top));
            if outside {
                tx.instant();
                tx.scroll_into_view_in(key(target, "composer-scroll"), marker.clone());
            } else {
                tx.live_only();
            }
        },
    ))
}

/// The height of the line under the room's composer, shown or empty, so the composer never
/// moves when someone starts typing.
const TYPING_LINE: f32 = 22.0;
/// The most faces the typing line stacks.
const TYPING_FACES: usize = 3;
const TYPING_FACE: f32 = 16.0;

/// Who is typing in the open room: their faces stacked, three dots pulsing in turn, and
/// "Ada is typing…", "Ada and Grace are typing…" or "Ada, Grace and 3 others are typing…".
fn typing_line(s: &State, p: &Palette) -> El {
    let line = row().key("typing").h(TYPING_LINE).w_full().px(4.0).gap(6.0).center_y();
    let people = s
        .active_account()
        .zip(s.active_room_id())
        .and_then(|(account, room_id)| account.typing.get(room_id))
        .filter(|people| !people.is_empty());
    let Some(people) = people else { return line };
    let mut faces = row().center_y();
    for (index, person) in people.iter().take(TYPING_FACES).enumerate() {
        let face =
            super::components::media_avatar(&person.id, &person.name, person.avatar.as_ref(), &s.media, TYPING_FACE)
                .radius(TYPING_FACE / 2.0)
                .border(1.5, p.bg);
        faces = faces.child(if index == 0 { face } else { face.ml(-5.0) });
    }
    let sentence = match people.as_slice() {
        [one] => tf!("typing.one", name = one.name),
        [first, second] => tf!("typing.two", first = first.name, second = second.name),
        [first, second, rest @ ..] => tf!("typing.many", first = first.name, second = second.name, count = rest.len()),
        [] => String::new(),
    };
    line.label(sentence.clone())
        .child(faces)
        .child(typing_dots(p))
        .child(text(sentence).font(font::META).color(p.text_muted).truncate().min_w(0.0))
}

/// Three dots that brighten one after another, on the renderer's clock: animating them builds
/// no view.
fn typing_dots(p: &Palette) -> El {
    const STOPS: &[(f32, f32)] = &[(0.0, 0.25), (0.2, 1.0), (0.4, 0.25), (1.0, 0.25)];
    let mut dots = row().gap(3.0).center_y().a11y_hidden();
    for index in 0..3u8 {
        let cycle = rune::Cycle { period: 1.2, phase: -f32::from(index) * 0.15, stops: STOPS, still: 0.6, fps: 30.0 };
        dots = dots
            .child(el().key(format!("typing-dot-{index}")).size(4.0, 4.0).radius(2.0).fill(p.text_muted).cycle(cycle));
    }
    dots
}

/// "Replying to Grace Hopper" with an excerpt of her message, and a way out.
fn reply_bar(target: ComposerTarget, sender: &str, excerpt: &str, p: &Palette) -> El {
    row()
        .key(key(target, "reply-bar"))
        .w_full()
        .px(12.0)
        .pt(8.0)
        .gap(10.0)
        .center_y()
        .child(icon(&icons::REPLY, 16.0).color(p.accent))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(tf!("composer.reply_to", name = sender)).font(font::META).color(p.text))
                .child(text(excerpt.to_owned()).font(font::META).color(p.text_muted).truncate()),
        )
        .child(
            icon_button(&icons::CLOSE, t!("composer.reply_cancel"), false, p.text_muted, p)
                .key(key(target, "reply-cancel"))
                .on_press(move |s: &mut State| s.cancel_reply(target)),
        )
}

/// "Editing your message", and a way out (as Escape).
fn edit_bar(target: ComposerTarget, p: &Palette) -> El {
    row()
        .key(key(target, "edit-bar"))
        .w_full()
        .px(12.0)
        .pt(8.0)
        .gap(10.0)
        .center_y()
        .child(icon(&icons::PENCIL, 16.0).color(p.accent))
        .child(text(t!("composer.editing")).font(font::META).color(p.text).grow(1.0))
        .child(
            icon_button(&icons::CLOSE, t!("composer.edit_cancel"), false, p.text_muted, p)
                .key(key(target, "edit-cancel"))
                .on_press(move |s: &mut State| s.cancel_edit(target)),
        )
}

/// A bar's button: it doesn't take focus, so the field keeps the caret.
fn bar_button(glyph: &'static rune::Icon, label: &str, active: bool, p: &Palette) -> El {
    icon_button(glyph, label, active, p.text_muted, p).focusable(false)
}

fn toolbar(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let mut bar = row()
        .key(key(target, "format-bar"))
        .w_full()
        .px(6.0)
        .pt(6.0)
        .gap(2.0)
        .flex_wrap()
        .center_y()
        .preserve_focus_on_press(true);
    for format in Format::ALL {
        if format.starts_group() {
            bar = bar.child(el().w(1.0).h(18.0).mx(4.0).fill(p.divider));
        }
        let (glyph, label) = format_look(format);
        bar = bar.child(
            bar_button(glyph, label, s.composer(target).is_active(format), p)
                .key(key(target, &format!("format-{}", format.id())))
                .on_press(move |s: &mut State| s.composer_mut(target).format(format)),
        );
    }
    bar
}

fn format_look(format: Format) -> (&'static rune::Icon, &'static str) {
    match format {
        Format::Bold => (&icons::BOLD, t!("format.bold")),
        Format::Italic => (&icons::ITALIC, t!("format.italic")),
        Format::Strike => (&icons::STRIKE, t!("format.strike")),
        Format::Code => (&icons::CODE, t!("format.code")),
        Format::Link => (&icons::LINK, t!("format.link")),
        Format::BulletList => (&icons::BULLETS, t!("format.bullets")),
        Format::NumberedList => (&icons::NUMBERS, t!("format.numbers")),
        Format::Quote => (&icons::QUOTE, t!("format.quote")),
        Format::CodeBlock => (&icons::CODE_BLOCK, t!("format.code_block")),
        Format::Spoiler => (&icons::EYE_OFF, t!("format.spoiler")),
        Format::Math => (&icons::SIGMA, t!("format.math")),
    }
}

/// How a pill looks in the field: a mention in the accent, `@room` in the warning colour, a
/// spoiler dark, a formula and a custom emoji on the surface.
fn pill_look(id: &str, p: &Palette) -> rune::PillStyle {
    let style = rune::PillStyle::new();
    match id {
        "everyone" => style.bg(p.warning.alpha(0.2)).fg(p.text),
        "spoiler" => style.bg(p.text_muted.alpha(0.35)).fg(p.text).icon(&icons::EYE_OFF),
        "math" => style.bg(p.hover).fg(p.text).icon(&icons::SIGMA),
        id if id.starts_with("emote:") => style.bg(p.hover).fg(p.text),
        _ => style.bg(p.accent.alpha(0.16)).fg(p.accent),
    }
}

/// "2 scheduled messages" above the room's composer, opening their list.
fn scheduled_bar(s: &State, target: ComposerTarget, p: &Palette) -> Option<El> {
    if target != ComposerTarget::Room {
        return None;
    }
    let here = s.scheduled_here();
    let failed = here.iter().filter(|message| message.failed.is_some()).count();
    let count = here.len();
    if count == 0 {
        return None;
    }
    // Any the server couldn't send come first: they wait for the user, not the clock.
    let (label, color) = match failed {
        0 => (crate::i18n::tp!("schedule.count", count), p.accent),
        failed => (crate::i18n::tp!("schedule.failed_count", failed), p.danger),
    };
    Some(
        row()
            .key(SCHEDULED_BAR)
            .px(10.0)
            .py(4.0)
            .mb(4.0)
            .gap(6.0)
            .center_y()
            .self_start()
            .radius(6.0)
            .hover(p.hover)
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .on_press(State::toggle_scheduled)
            .child(icon(&icons::CLOCK, 14.0).color(color))
            .child(text(label).font(font::META).color(color)),
    )
}

/// The key of the bar listing the room's scheduled messages, which their list anchors to.
pub const SCHEDULED_BAR: &str = "scheduled-bar";

fn link_entry(target: ComposerTarget, link: &rune::TextEdit, p: &Palette) -> El {
    let field = text_input(link, font::BODY)
        .key(key(target, "link-url"))
        .single_line()
        .color(p.text)
        .grow(1.0)
        .px(10.0)
        .py(6.0)
        .radius(6.0)
        .fill(p.bg)
        .border(1.0, p.divider)
        .on_edit(move |s: &mut State| s.composer_mut(target).link.get_or_insert_default())
        .on_submit(move |s: &mut State, _| s.composer_mut(target).confirm_link())
        .on_input_key(move |s: &mut State, key, _| {
            let cancel = key.key == Key::Escape;
            if cancel {
                s.composer_mut(target).cancel_link();
            }
            cancel
        });
    row()
        .w_full()
        .px(10.0)
        .pt(6.0)
        .gap(6.0)
        .center_y()
        .child(icon(&icons::LINK, 16.0).color(p.text_muted))
        .child(field)
        .child(
            icon_button(&icons::CHECK, t!("composer.link_apply"), false, p.text_muted, p)
                .key(key(target, "link-apply"))
                .on_press(move |s: &mut State| s.composer_mut(target).confirm_link()),
        )
        .child(
            icon_button(&icons::CLOSE, t!("composer.link_cancel"), false, p.text_muted, p)
                .key(key(target, "link-cancel"))
                .on_press(move |s: &mut State| s.composer_mut(target).cancel_link()),
        )
}

fn attachments(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let mut chips = row().w_full().px(10.0).pb(4.0).gap(6.0).flex_wrap();
    for (index, upload) in s.composer(target).attachments.iter().enumerate() {
        let name = upload.name();
        chips = chips.child(
            row()
                .key(key(target, &format!("attachment-{index}")))
                .h(30.0)
                .pl(10.0)
                .pr(2.0)
                .gap(6.0)
                .center_y()
                .radius(6.0)
                .fill(p.bg)
                .border(1.0, p.divider)
                .child(icon(&icons::FILE, 14.0).color(p.text_muted))
                .child(text(name.clone()).font(font::META).color(p.text).max_w(220.0).truncate())
                .child(
                    icon_button(&icons::CLOSE, &tf!("composer.remove", name = name), false, p.text_muted, p)
                        .size(24.0, 24.0)
                        .on_press(move |s: &mut State| s.composer_mut(target).remove_attachment(index)),
                ),
        );
    }
    chips
}

/// The element key of a composer's emoji button, which its picker anchors to.
pub fn emoji_button(target: ComposerTarget) -> String {
    key(target, "emoji")
}

/// The element key of a composer's sticker button, which the sticker picker anchors to.
pub fn sticker_button(target: ComposerTarget) -> String {
    key(target, "stickers")
}

/// The element key of the button beside send, which the schedule menu anchors to.
pub fn schedule_button(target: ComposerTarget) -> String {
    key(target, "schedule")
}

/// The element key of a composer's frame, which its suggestions sit on.
pub fn frame_key(target: ComposerTarget) -> String {
    key(target, "composer-frame")
}

fn bottom_bar(s: &State, target: ComposerTarget, p: &Palette) -> El {
    let c = s.composer(target);
    let can_send = !c.is_empty();
    let send = el()
        .key(key(target, "send"))
        .size(32.0, 28.0)
        .radius(6.0)
        .center()
        .fill(if can_send { p.accent } else { Color::TRANSPARENT })
        .role("button")
        .cursor(rune::Cursor::Pointer)
        .label(t!("composer.send"))
        .child(icon(&icons::SEND, 16.0).color(if can_send { p.on_accent } else { p.text_muted }));
    let send = if can_send { send.on_press_tx(move |s: &mut State, tx| s.send_message(target, tx)) } else { send };
    // Beside send: send later. Right-clicking send opens the same menu.
    let send = if s.can_schedule(target) {
        send.on_secondary_press(move |s: &mut State, _, _| s.toggle_popover(Popover::Schedule(target)))
    } else {
        send
    };
    let schedule = s.can_schedule(target).then(|| {
        el().key(schedule_button(target))
            .size(18.0, 28.0)
            .radius(6.0)
            .center()
            .hover(p.hover.alpha(p.hover.a * 2.0))
            .role("button")
            .cursor(rune::Cursor::Pointer)
            .label(t!("schedule.button"))
            .help(t!("schedule.button"))
            .focusable(false)
            .child(icon(&icons::CHEVRON_DOWN, 14.0).color(p.text_muted))
            .on_press(move |s: &mut State| s.toggle_popover(Popover::Schedule(target)))
    });
    let separator = || el().w(1.0).h(18.0).mx(4.0).fill(p.divider);
    let picking_emoji = s.popover == Some(Popover::Emoji(EmojiFor::Composer(target)));
    let voice = if voice_memos_supported() {
        bar_button(&icons::MIC, t!("composer.voice"), false, p)
            .key(key(target, "voice"))
            .on_press(move |s: &mut State| s.start_voice_memo(target))
    } else {
        bar_button(&icons::MIC, t!("composer.voice_unsupported"), false, p)
            .key(key(target, "voice"))
            .opacity(0.4)
            .cursor(rune::Cursor::NotAllowed)
            .help(t!("composer.voice_unsupported"))
    };
    // An edit changes the text only: no attachments, voice memos or polls while one runs.
    let editing = c.editing.is_some();
    let mut bar = row()
        .key(key(target, "composer-bar"))
        .w_full()
        .px(8.0)
        .pb(8.0)
        .gap(2.0)
        .center_y()
        .preserve_focus_on_press(true);
    if !editing {
        bar = bar
            .child(
                bar_button(&icons::PAPERCLIP, t!("composer.attach"), false, p)
                    .key(key(target, "attach"))
                    .on_press(move |s: &mut State| s.pick_attachments(target)),
            )
            .child(separator());
    }
    bar = bar
        .child(
            bar_button(&icons::SMILE, t!("composer.emoji"), picking_emoji, p)
                .key(emoji_button(target))
                .on_press(move |s: &mut State| s.open_composer_emoji(target)),
        )
        .children((!editing).then(|| {
            bar_button(&icons::STICKER, t!("composer.stickers"), s.popover == Some(Popover::Stickers(target)), p)
                .key(sticker_button(target))
                .help(t!("composer.stickers"))
                .on_press(move |s: &mut State| s.open_stickers(target))
        }))
        .child(
            el().key(key(target, "formatting"))
                .size(28.0, 28.0)
                .radius(6.0)
                .center()
                .fill(if c.formatting { p.hover.alpha(p.hover.a * 3.0) } else { Color::TRANSPARENT })
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .label(t!("composer.formatting"))
                .checked(c.formatting)
                .child(text("Aa").font(font::BODY_STRONG).color(if c.formatting { p.text } else { p.text_muted }))
                .on_press(move |s: &mut State| s.composer_mut(target).toggle_formatting()),
        );
    if !editing {
        bar = bar
            .child(separator())
            .child(voice)
            .child(
                bar_button(&icons::POLL, t!("composer.poll"), false, p)
                    .key(key(target, "poll"))
                    .help(t!("composer.poll"))
                    .on_press(move |s: &mut State| s.open_poll_form(target)),
            )
            .child(
                bar_button(&icons::MAP_PIN, t!("composer.location"), false, p)
                    .key(key(target, "location"))
                    .help(t!("composer.location"))
                    .on_press(move |s: &mut State| s.open_location(target)),
            );
    }
    bar.child(el().grow(1.0)).child(send).children(schedule)
}

/// Voice memos record on every desktop, through the app's own sound (`platform::sound`).
pub fn voice_memos_supported() -> bool {
    true
}

/// How far the voice glow reaches past the composer's frame, in points.
const GLOW_REACH: f32 = 6.0;

/// While a voice memo records, a soft blue glow around the composer that swells with the voice:
/// a faint halo always, and over it a brighter one whose opacity follows the recorder's level,
/// bound live, so the view never rebuilds for it. Under reduced motion the brighter halo holds
/// still, half lit.
fn voice_glow(target: ComposerTarget, memo: &crate::state::VoiceMemo, still: bool, p: &Palette) -> El {
    let halo = |reach: f32, sigma: f32, alpha: f32| {
        el().pin(Some(-reach), Some(-reach), Some(-reach), Some(-reach))
            .radius(10.0 + reach)
            .fill(p.accent.alpha(alpha))
            .blur(sigma)
    };
    let voice = halo(GLOW_REACH, 12.0, 0.8).key(key(target, "voice-glow-level"));
    let voice = if still {
        voice.opacity(0.5)
    } else {
        voice.live_opacity(memo.recorder.live_level().smooth(rune::Spring::new(0.12, 0.0)))
    };
    el().key(key(target, "voice-glow"))
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .a11y_hidden()
        .child(halo(2.0, 6.0, 0.35))
        .child(voice)
}

/// The red dot of a recording, lit like a studio's recording light: the lamp dims a touch and
/// a soft halo around it swells and fades, slowly, on the renderer's clock (no rebuilds).
/// Under reduced motion both hold still, lit.
fn recording_light(p: &Palette) -> El {
    const PERIOD: f32 = 1.6;
    const LAMP: &[(f32, f32)] = &[(0.0, 1.0), (0.5, 0.72), (1.0, 1.0)];
    const HALO: &[(f32, f32)] = &[(0.0, 0.55), (0.5, 0.12), (1.0, 0.55)];
    let breathe =
        |stops: &'static [(f32, f32)], still: f32| rune::Cycle { period: PERIOD, phase: 0.0, stops, still, fps: 30.0 };
    el().size(10.0, 10.0)
        .a11y_hidden()
        .child(
            el().key("recording-halo")
                .absolute(-7.0, -7.0)
                .size(24.0, 24.0)
                .radial_gradient(
                    [0.5, 0.5],
                    12.0,
                    [(0.0, p.danger.alpha(0.9)), (0.42, p.danger.alpha(0.45)), (1.0, p.danger.alpha(0.0))],
                )
                .cycle(breathe(HALO, 0.35)),
        )
        .child(
            el().key("recording-lamp")
                .absolute(0.0, 0.0)
                .size(10.0, 10.0)
                .radius(5.0)
                .fill(p.danger)
                .cycle(breathe(LAMP, 1.0)),
        )
}

/// While a voice memo records: a red dot, the time so far, the input level, and ways to drop or
/// send it.
fn recording(target: ComposerTarget, memo: &crate::state::VoiceMemo, p: &Palette) -> El {
    let seconds = memo.recorder.elapsed().max(0.0) as u64;
    row()
        .key(key(target, "voice-recording"))
        .w_full()
        .flex_wrap()
        .px(14.0)
        .py(14.0)
        .gap(10.0)
        .center_y()
        .child(recording_light(p))
        .child(text(t!("composer.recording")).font(font::BODY).color(p.text))
        .child(text(format!("{}:{:02}", seconds / 60, seconds % 60)).font(font::BODY_STRONG).color(p.text))
        .child(el().w(120.0).h(6.0).radius(3.0).fill(p.divider).child(
            el().w(120.0).h(6.0).radius(3.0).fill(p.accent).origin(0.0, 0.5).live_scale(&memo.recorder.live_level()),
        ))
        .child(el().grow(1.0))
        .child(
            icon_button(&icons::CLOSE, t!("composer.voice_cancel"), false, p.text_muted, p)
                .key(key(target, "voice-cancel"))
                .on_press(move |s: &mut State| s.cancel_voice_memo(target)),
        )
        .child(
            icon_button(&icons::SEND, t!("composer.voice_send"), false, p.accent, p)
                .key(key(target, "voice-send"))
                .on_press_tx(move |s: &mut State, tx| s.send_voice_memo(target, tx)),
        )
}
