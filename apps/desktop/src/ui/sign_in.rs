//! The sign-in screen: a card under the app's logo and name, in two steps: 1. homeserver, 2. sign
//! in (browser or password, whichever the server uses), or create an account, or sign in with a
//! QR code from a device already signed in. Used for the first account and for adding more.

use matrix_core::{Homeserver, RegistrationStep};
use rune::{Color, El, ScrollSpec, col, icon, row, text};

use super::account_dialogs::{QrSide, qr_login_action, qr_login_steps};
use super::backdrop;
use super::components::{ButtonKind, button, field_input, labelled};
use super::icons;
use super::theme::{Palette, font};
use crate::i18n::{t, tf};
use crate::state::{Asked, QrLogin, SignInStep, State};

pub fn sign_in(s: &State, p: &Palette) -> El {
    col().key("sign-in-screen").w_full().h_full().padding(24.0).center().child(card(s, p))
}

/// The same sign-in form over the conversation when adding another account.
pub fn dialog(s: &State, window: [f32; 2], dark: bool, p: &Palette) -> El {
    col()
        .key("sign-in-modal")
        .size(window[0] * 0.9, window[1] * 0.9)
        .padding(1.0)
        .radius(14.0)
        .fill(p.bg)
        .border(1.0, p.text_muted.alpha(0.35))
        .role("dialog")
        .label(t!("sign_in.add_title"))
        .shadow(16.0, 48.0, Color::BLACK.alpha(0.5))
        .child(backdrop::behind(s, sign_in(s, p), dark, p).radius(13.0).clip_rounded(13.0))
}

fn card(s: &State, p: &Palette) -> El {
    let form = &s.sign_in;
    let adding = !s.accounts.is_empty();

    let mut card = col()
        .key("sign-in-card")
        .w_full()
        .max_w(440.0)
        .padding(32.0)
        .gap(20.0)
        .radius(14.0)
        .fill(p.surface)
        .border(1.0, p.divider)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(brand(p));

    card = match &form.step {
        SignInStep::Server { .. } | SignInStep::Discovering(_) => card.child(server_step(s, adding, p)),
        SignInStep::Browser { server, request, url, create, .. } => card
            .child(server_row(server, p))
            .child(browser_step(s, server, request.is_some(), url.is_some(), *create, p)),
        SignInStep::Password { server, submitting, .. } => {
            card.child(server_row(server, p)).child(password_step(s, server, submitting.is_some(), p))
        }
        SignInStep::Register { server, request, asked, .. } => {
            card.child(server_row(server, p)).child(register_step(s, server, request.is_some(), asked.as_ref(), p))
        }
        SignInStep::Qr { server, flow } => card.child(server_row(server, p)).child(qr_step(flow, p)),
    };
    if let Some(error) = form.error() {
        card = card.child(text(error.to_owned()).key("sign-in-error").font(font::BODY).color(p.danger).wrap());
    }
    if adding {
        card = card.child(
            button(t!("common.cancel"), ButtonKind::Secondary, true, p)
                .key("cancel")
                .w_full()
                .on_press(State::close_modal),
        );
    }
    card
}

/// The app's logo and name, heading the card.
fn brand(p: &Palette) -> El {
    row()
        .key("brand")
        .gap(10.0)
        .center_y()
        .child(icon(&icons::LOGO, 32.0).color(p.accent))
        .child(text(t!("app.name")).font(font::WORKSPACE).color(p.text))
}

fn server_step(s: &State, adding: bool, p: &Palette) -> El {
    let discovering = matches!(s.sign_in.step, SignInStep::Discovering(_));
    let title = if adding { t!("sign_in.add_title") } else { t!("sign_in.title") };
    let field = field_input(&s.sign_in.homeserver, "matrix.org", p)
        .key("homeserver")
        .on_edit(|s: &mut State| &mut s.sign_in.homeserver)
        .on_submit(|s: &mut State, _| s.submit_server());
    let label = if discovering {
        tf!("sign_in.looking_up", server = s.sign_in.homeserver.text.trim())
    } else {
        t!("common.continue").to_owned()
    };
    let mut step = col().w_full().gap(16.0).child(text(title).font(font::TITLE).color(p.text));
    // Accounts signed out softly, whose data waits here: signing in as one takes it back.
    if !s.suspended.is_empty() {
        let mut list = col()
            .key("suspended")
            .w_full()
            .gap(6.0)
            .child(text(t!("sign_in.signed_out_heading")).font(font::SECTION).color(p.text_muted));
        for id in &s.suspended {
            let (resume, forget) = (id.clone(), id.clone());
            list = list.child(
                row()
                    .w_full()
                    .gap(12.0)
                    .center_y()
                    .child(text(id.as_str().to_owned()).font(font::BODY).color(p.text).grow(1.0).min_w(0.0).truncate())
                    .child(
                        link(t!("session_ended.sign_in"), p.accent)
                            .key(format!("resume-{id}"))
                            .on_press_tx(crate::state::navigate(move |s| s.sign_in_as(&resume))),
                    )
                    .child(
                        link(t!("common.remove"), p.text_muted)
                            .key(format!("forget-{id}"))
                            .on_press(move |s: &mut State| s.forget_suspended(&forget)),
                    ),
            );
        }
        step = step.child(list);
    }
    step.child(labelled(t!("sign_in.homeserver"), field, p))
        .child(text(t!("sign_in.homeserver_hint")).font(font::META).color(p.text_muted).wrap())
        .child(
            button(&label, ButtonKind::Primary, !discovering, p)
                .key("continue")
                .w_full()
                .on_press(State::submit_server),
        )
}

/// The resolved homeserver, where it lives if that differs, and a way back to step 1.
fn server_row(server: &Homeserver, p: &Palette) -> El {
    let host = server.url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let mut names = col().grow(1.0).min_w(0.0).child(
        text(server.display_name().to_owned()).key("server-name").font(font::BODY_STRONG).color(p.text).truncate(),
    );
    if host != server.display_name() {
        names = names.child(text(tf!("sign_in.via", host = host)).font(font::META).color(p.text_muted).truncate());
    }
    row().w_full().gap(10.0).center_y().child(icon(&icons::GLOBE, 18.0).color(p.text_muted)).child(names).child(
        text(t!("common.change")).key("change-server").font(font::BODY).color(p.accent).on_press(State::change_server),
    )
}

fn browser_step(s: &State, server: &Homeserver, waiting: bool, opened: bool, create: bool, p: &Palette) -> El {
    let message = match (waiting, create) {
        (true, false) => tf!("sign_in.browser_waiting", server = server.display_name()),
        (true, true) => tf!("register.browser_waiting", server = server.display_name()),
        (false, _) => t!("sign_in.browser_failed").to_owned(),
    };
    let title = if create { t!("register.title") } else { t!("sign_in.browser_title") };
    let mut step = col()
        .w_full()
        .gap(12.0)
        .child(text(title).font(font::TITLE).color(p.text))
        .child(text(message).font(font::BODY).color(p.text_muted).wrap());
    if !waiting {
        step = step.child(
            button(t!("sign_in.try_again"), ButtonKind::Primary, true, p)
                .key("retry-browser")
                .w_full()
                .on_press(State::retry_browser),
        );
    } else if opened {
        step = step.child(
            button(t!("sign_in.reopen"), ButtonKind::Secondary, true, p)
                .key("reopen-browser")
                .w_full()
                .on_press_tx(State::reopen_browser),
        );
    }
    if server.methods.password {
        step = step.child(alternative(t!("sign_in.use_password"), "use-password", State::use_password, p));
    }
    if create {
        step.child(alternative(t!("register.sign_in_instead"), "sign-in-instead", State::retry_browser, p))
    } else {
        step.children(other_ways(s, server, p))
    }
}

/// The other ways in step 2 offers where the server has them: creating an account, and a QR code.
/// Signing in again as a suspended account offers neither: both start a new store.
fn other_ways(s: &State, server: &Homeserver, p: &Palette) -> Vec<El> {
    let mut ways = Vec::new();
    if !s.sign_in.starts_afresh() {
        return ways;
    }
    if server.methods.qr {
        ways.push(alternative(t!("sign_in.use_qr"), "use-qr", State::start_qr_sign_in, p));
    }
    if server.methods.create_account {
        ways.push(alternative(t!("register.create"), "create-account", State::start_registration, p));
    }
    ways
}

fn password_step(s: &State, server: &Homeserver, submitting: bool, p: &Palette) -> El {
    let username = field_input(&s.sign_in.username, "alice", p)
        .key("username")
        .on_edit(|s: &mut State| &mut s.sign_in.username)
        .on_submit(|s: &mut State, _| s.submit_password());
    let password = field_input(&s.sign_in.password, t!("sign_in.password"), p)
        .key("password")
        .on_edit(|s: &mut State| &mut s.sign_in.password)
        .on_submit(|s: &mut State, _| s.submit_password());
    let mut step = col()
        .w_full()
        .gap(14.0)
        .child(text(t!("sign_in.password_title")).font(font::TITLE).color(p.text))
        .child(labelled(t!("sign_in.username"), username, p))
        .child(labelled(t!("sign_in.password"), password, p))
        .child(
            button(
                if submitting { t!("sign_in.signing_in") } else { t!("sign_in.sign_in") },
                ButtonKind::Primary,
                !submitting,
                p,
            )
            .key("sign-in")
            .w_full()
            .on_press(State::submit_password),
        );
    if server.methods.oauth {
        step = step.child(alternative(t!("sign_in.use_browser"), "use-browser", State::retry_browser, p));
    }
    step.children(other_ways(s, server, p))
}

/// Creating an account on a password server: the form, then whatever the server asks.
fn register_step(s: &State, server: &Homeserver, running: bool, asked: Option<&Asked>, p: &Palette) -> El {
    let step = col().w_full().gap(14.0);
    let answered = asked.is_some_and(|asked| asked.answered);
    let step = match asked.map(|asked| &asked.step) {
        None => {
            let username = field_input(&s.sign_in.username, "alice", p)
                .key("username")
                .on_edit(|s: &mut State| &mut s.sign_in.username)
                .on_submit(|s: &mut State, _| s.submit_registration());
            let password = field_input(&s.sign_in.password, t!("sign_in.password"), p)
                .key("password")
                .on_edit(|s: &mut State| &mut s.sign_in.password)
                .on_submit(|s: &mut State, _| s.submit_registration());
            let confirm = field_input(&s.sign_in.confirm, t!("sign_in.password"), p)
                .key("password-confirm")
                .on_edit(|s: &mut State| &mut s.sign_in.confirm)
                .on_submit(|s: &mut State, _| s.submit_registration());
            step.child(text(t!("register.title")).font(font::TITLE).color(p.text))
                .child(labelled(t!("sign_in.username"), username, p))
                .child(labelled(t!("sign_in.password"), password, p))
                .child(labelled(t!("password.confirm"), confirm, p))
                .child(
                    button(
                        if running { t!("register.creating") } else { t!("register.submit") },
                        ButtonKind::Primary,
                        !running,
                        p,
                    )
                    .key("register")
                    .w_full()
                    .on_press(State::submit_registration),
                )
        }
        Some(RegistrationStep::Token) => {
            let field = field_input(&s.sign_in.token, t!("register.token"), p)
                .key("registration-token")
                .on_edit(|s: &mut State| &mut s.sign_in.token)
                .on_submit(|s: &mut State, _| s.submit_registration_token());
            step.child(text(t!("register.token_title")).font(font::TITLE).color(p.text))
                .child(muted(&tf!("register.token_hint", server = server.display_name()), p))
                .child(labelled(t!("register.token"), field, p))
                .child(
                    button(t!("common.continue"), ButtonKind::Primary, !answered, p)
                        .key("registration-token-submit")
                        .w_full()
                        .on_press(State::submit_registration_token),
                )
        }
        Some(RegistrationStep::Email) => {
            let field = field_input(&s.sign_in.email, "alice@example.org", p)
                .key("registration-email")
                .label(t!("register.email"))
                .on_edit(|s: &mut State| &mut s.sign_in.email)
                .on_submit(|s: &mut State, _| s.submit_registration_email());
            step.child(text(t!("register.email_title")).font(font::TITLE).color(p.text))
                .child(muted(&tf!("register.email_hint", server = server.display_name()), p))
                .child(labelled(t!("register.email"), field, p))
                .child(
                    button(t!("common.continue"), ButtonKind::Primary, !answered, p)
                        .key("registration-email-submit")
                        .w_full()
                        .on_press(State::submit_registration_email),
                )
        }
        Some(RegistrationStep::EmailSent { address, unconfirmed }) => {
            let mut sent = step
                .child(text(t!("register.email_sent_title")).font(font::TITLE).color(p.text))
                .child(muted(&tf!("register.email_sent", address = address), p));
            if *unconfirmed {
                sent = sent.child(
                    text(tf!("register.email_unconfirmed", server = server.display_name()))
                        .key("registration-email-unconfirmed")
                        .font(font::BODY)
                        .color(p.danger)
                        .wrap(),
                );
            }
            sent.child(
                button(t!("common.continue"), ButtonKind::Primary, !answered, p)
                    .key("registration-continue")
                    .w_full()
                    .on_press(State::continue_registration),
            )
        }
        Some(RegistrationStep::Terms(policies)) => {
            let list = policies.iter().enumerate().fold(col().w_full().gap(6.0), |list, (index, policy)| {
                let url = policy.url.clone();
                list.child(
                    row().gap(8.0).center_y().child(icon(&icons::EXTERNAL, 16.0).color(p.accent)).child(
                        link(&policy.name, p.accent)
                            .key(format!("policy-{index}"))
                            .on_press_tx(move |s: &mut State, tx| s.open_link(&url, tx)),
                    ),
                )
            });
            step.child(text(t!("register.terms_title")).font(font::TITLE).color(p.text))
                .child(muted(&tf!("register.terms_hint", server = server.display_name()), p))
                .child(list)
                .child(
                    button(t!("register.agree"), ButtonKind::Primary, !answered, p)
                        .key("registration-agree")
                        .w_full()
                        .on_press(State::agree_to_terms),
                )
        }
        Some(RegistrationStep::Browser { .. }) => step
            .child(text(t!("sign_in.browser_title")).font(font::TITLE).color(p.text))
            .child(muted(&tf!("register.browser_stage", server = server.display_name()), p))
            .child(
                button(t!("common.continue"), ButtonKind::Primary, !answered, p)
                    .key("registration-continue")
                    .w_full()
                    .on_press(State::continue_registration),
            )
            .child(
                button(t!("register.reopen"), ButtonKind::Secondary, true, p)
                    .key("registration-reopen")
                    .w_full()
                    .on_press_tx(State::reopen_registration_page),
            ),
    };
    step.child(alternative(t!("register.sign_in_instead"), "sign-in-instead", State::sign_in_instead, p))
}

/// Signing in with a QR code: this device shows one for a device already signed in to scan.
fn qr_step(flow: &QrLogin, p: &Palette) -> El {
    let mut step = col()
        .w_full()
        .gap(14.0)
        .child(text(t!("sign_in.qr_title")).font(font::TITLE).color(p.text))
        .child(qr_login_steps(flow, QrSide::SigningIn, p));
    if let Some(action) = qr_login_action(flow, QrSide::SigningIn, p) {
        step = step.child(action.w_full());
    }
    if flow.request.is_none() {
        step = step.child(
            button(t!("sign_in.try_again"), ButtonKind::Primary, true, p)
                .key("retry-qr")
                .w_full()
                .on_press(State::start_qr_sign_in),
        );
    }
    step.child(alternative(t!("sign_in.other_way"), "sign-in-instead", State::sign_in_instead, p))
}

fn muted(message: &str, p: &Palette) -> El {
    text(message.to_owned()).font(font::BODY).color(p.text_muted).wrap()
}

/// A quiet link to the other sign-in method.
fn alternative(label: &str, key: &str, action: fn(&mut State), p: &Palette) -> El {
    row().w_full().center().child(link(label, p.accent).key(key.to_owned()).on_press(action))
}

/// Text that acts as a button: a pointer, focus, and the button role.
fn link(label: &str, color: rune::Color) -> El {
    text(label.to_owned()).font(font::BODY).color(color).role("button").cursor(rune::Cursor::Pointer).focusable(true)
}
