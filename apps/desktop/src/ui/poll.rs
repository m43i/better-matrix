//! Polls: the card a poll shows as in the timeline (vote by pressing an answer; the results show
//! as people vote, or once it ends), and the dialog that writes one.

use matrix_core::{Poll, TimelineKey};
use rune::{El, col, el, icon, row, text};

use super::components::{ButtonKind, button, choice_row, dialog, field_input, icon_button, labelled};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{MAX_ANSWERS, State};

/// A poll in the timeline. `own`: the user started it, so may end it.
pub fn card(poll: &Poll, event_id: &str, own: bool, key: &TimelineKey, p: &Palette) -> El {
    let results = poll.disclosed || poll.ended;
    let total: u32 = poll.answers.iter().map(|answer| answer.votes).sum();
    let top = poll.answers.iter().map(|answer| answer.votes).max().unwrap_or(0);
    // A local echo has no event ID to vote on yet.
    let votable = !poll.ended && !event_id.starts_with("local:");
    let mut answers = col().w_full().gap(6.0);
    for answer in &poll.answers {
        let won = poll.ended && answer.votes > 0 && answer.votes == top;
        let mark = el()
            .size(18.0, 18.0)
            .radius(if poll.max_selections > 1 { 4.0 } else { 9.0 })
            .border(2.0, if answer.mine { p.accent } else { p.text_muted })
            .center()
            .children(answer.mine.then(|| icon(&icons::CHECK, 12.0).color(p.accent)));
        let mut head = row().w_full().gap(10.0).center_y().child(mark).child(
            text(answer.text.clone())
                .font(if won { font::BODY_STRONG } else { font::BODY })
                .color(p.text)
                .grow(1.0)
                .min_w(0.0)
                .wrap_anywhere(),
        );
        if results {
            head = head.child(text(answer.votes.to_string()).font(font::META).color(p.text_muted));
        }
        let mut entry = col()
            .key(format!("poll-{event_id}-{}", answer.id))
            .w_full()
            .px(10.0)
            .py(8.0)
            .gap(6.0)
            .radius(8.0)
            .border(1.0, if answer.mine { p.accent } else { p.divider })
            .child(head);
        if results {
            // The share of the votes, as a bar.
            let share = row()
                .w_full()
                .h(4.0)
                .radius(2.0)
                .fill(p.divider)
                .clip_rounded(2.0)
                .child(el().h_full().grow(answer.votes as f32).fill(if won { p.success } else { p.accent }))
                .child(
                    el().h_full().grow(total.saturating_sub(answer.votes) as f32 + if total == 0 { 1.0 } else { 0.0 }),
                );
            entry = entry.child(share);
        }
        if votable {
            let (poll, key, event_id, id) = (poll.clone(), key.clone(), event_id.to_owned(), answer.id.clone());
            entry = entry
                .hover(p.hover.alpha(p.hover.a * 2.0))
                .role(if poll.max_selections > 1 { "checkbox" } else { "radio" })
                .checked(answer.mine)
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .label(answer.text.clone())
                .on_press(move |s: &mut State| s.vote(key.clone(), &poll, &event_id, &id));
        }
        answers = answers.child(entry);
    }
    let status = if poll.ended {
        tf!("poll.final", count = poll.voters)
    } else if poll.disclosed {
        tf!("poll.votes", count = poll.voters)
    } else {
        tf!("poll.hidden", count = poll.voters)
    };
    let mut footer = row()
        .w_full()
        .gap(10.0)
        .center_y()
        .child(text(status).font(font::META).color(p.text_muted).grow(1.0).min_w(0.0).wrap_anywhere());
    if own && votable {
        let (key, event_id, question) = (key.clone(), event_id.to_owned(), poll.question.clone());
        footer = footer.child(
            text(t!("poll.end"))
                .key(format!("poll-end-{event_id}"))
                .font(font::META)
                .color(p.accent)
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .on_press(move |s: &mut State| s.end_poll(key.clone(), &event_id, &question)),
        );
    }
    col()
        .key(format!("poll-{event_id}"))
        .max_w(460.0)
        .mt(4.0)
        .padding(14.0)
        .gap(10.0)
        .radius(10.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .child(row().w_full().gap(8.0).child(icon(&icons::POLL, 18.0).color(p.accent)).child(
            text(poll.question.clone()).font(font::BODY_STRONG).color(p.text).grow(1.0).min_w(0.0).wrap_anywhere(),
        ))
        .child(answers)
        .child(footer)
}

/// The dialog writing a poll: its question, its answers (two to twenty), when its results show.
pub fn form(s: &State, p: &Palette) -> El {
    let form = &s.poll_form;
    let question = field_input(&form.question, t!("poll.question_placeholder"), p)
        .key("poll-question")
        .on_edit(|s: &mut State| &mut s.poll_form.question);
    let mut answers = col().w_full().gap(8.0);
    for (index, answer) in form.answers.iter().enumerate() {
        let field = field_input(answer, &tf!("poll.answer_placeholder", number = index + 1), p)
            .key(format!("poll-answer-{index}"))
            .on_edit(move |s: &mut State| s.poll_form.answer_mut(index))
            .grow(1.0);
        let mut line = row().w_full().gap(6.0).center_y().child(field);
        if form.answers.len() > 2 {
            line = line.child(
                icon_button(&icons::CLOSE, t!("poll.remove_answer"), false, p.text_muted, p)
                    .key(format!("poll-remove-{index}"))
                    .on_press(move |s: &mut State| s.poll_form.remove_answer(index)),
            );
        }
        answers = answers.child(line);
    }
    if form.answers.len() < MAX_ANSWERS {
        answers = answers.child(
            row()
                .key("poll-add-answer")
                .gap(6.0)
                .center_y()
                .role("button")
                .cursor(rune::Cursor::Pointer)
                .focusable(true)
                .child(icon(&icons::PLUS, 16.0).color(p.accent))
                .child(text(t!("poll.add_answer")).font(font::BODY).color(p.accent))
                .on_press(|s: &mut State| s.poll_form.add_answer()),
        );
    }
    let kinds = [
        (true, t!("poll.disclosed"), t!("poll.disclosed_hint")),
        (false, t!("poll.undisclosed"), t!("poll.undisclosed_hint")),
    ]
    .into_iter()
    .fold(col().w_full().gap(8.0), |list, (disclosed, title, hint)| {
        list.child(
            choice_row(title, Some(hint), form.disclosed == disclosed, p)
                .key(format!("poll-kind-{disclosed}"))
                .on_press(move |s: &mut State| s.poll_form.disclosed = disclosed),
        )
    });
    let content = col()
        .w_full()
        .gap(16.0)
        .child(labelled(t!("poll.question"), question, p))
        .child(labelled(t!("poll.answers"), answers, p))
        .child(kinds);
    dialog(
        "poll-dialog",
        t!("poll.title"),
        content,
        vec![
            button(t!("common.cancel"), ButtonKind::Secondary, true, p).key("poll-cancel").on_press(State::close_modal),
            button(t!("poll.create"), ButtonKind::Primary, form.ready(), p)
                .key("poll-create")
                .on_press_tx(State::send_poll),
        ],
        p,
    )
}
