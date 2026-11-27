//! The widget window's own process (`better-matrix --widget-window`): one window holding the
//! system's web view, driven by the app over stdin and stdout (see the parent module).
//!
//! - The first line must be `open`; the window shows its page then. The origin of its URL is
//!   the page's approved origin. Another `open` loads the page anew (a navigation, so a real
//!   reload even at the same address), dropping messages not yet delivered.
//! - The page's messages reach the app through a script injected into the main frame before
//!   the page's own ([`shim`]): it takes over `window.postMessage`, and what the page posts that
//!   the bridge lets out goes to the app instead of back into the page (an integration manager
//!   would otherwise read its own request as the answer). Each goes through the web view's IPC
//!   with a token made for this window, which only the shim knows; this side drops what lacks it,
//!   what comes from another origin, and what the bridge doesn't let out ([`accept`]).
//! - Messages for the page wait until it has loaded, then go in with `window.postMessage`
//!   ([`post_script`]), as a parent window would post them, addressed to the approved origin, and
//!   only while the document showing is on that origin ([`delivers_to`]); the bridge never lets
//!   them out again. Each time the page starts loading a document, the app hears `loading`, and
//!   `loaded` once it has.
//! - The page stays on its origin: navigating elsewhere goes to the browser instead.
//! - No camera, microphone or screen for widgets here: meetings open in the browser.
//! - Links the page opens in a new window go to the app, for the browser.

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::Write;
use std::process::ExitCode;
use std::rc::Rc;

use serde_json::Value;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, WebViewBuilder};

use super::{Bridge, FromHelper, ToHelper};

/// The window's size when it opens, in points.
const SIZE: (f64, f64) = (1024.0, 768.0);

/// What wakes the window's event loop.
#[derive(Debug)]
enum Wake {
    /// The app said something.
    App(ToHelper),
    /// The page started loading (`false`) or finished (`true`).
    Loaded(bool),
    /// The app is gone: stdin ended.
    Orphaned,
}

/// Runs the widget window until it closes or the app goes.
pub fn run() -> ExitCode {
    let mut first = String::new();
    if std::io::stdin().read_line(&mut first).is_err() {
        return ExitCode::FAILURE;
    }
    let Some(ToHelper::Open { url, title, bridge }) = ToHelper::parse(&first) else {
        eprintln!("better-matrix: a widget window must be opened first");
        return ExitCode::FAILURE;
    };
    match show(&url, &title, bridge) {
        Ok(never) => match never {},
        Err(_) => {
            say(&FromHelper::Failed);
            ExitCode::FAILURE
        }
    }
}

/// Tells the app something, on stdout.
fn say(message: &FromHelper) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(message.line().as_bytes());
    let _ = out.flush();
}

/// A secret made for one window, which the shim proves its messages with.
fn token() -> Result<String, getrandom::Error> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Shows the window and runs its event loop, which ends the process.
fn show(url: &str, title: &str, bridge: Bridge) -> Result<std::convert::Infallible, Box<dyn std::error::Error>> {
    let approved = Rc::new(RefCell::new(origin(url).ok_or("the widget's address has no origin")?));
    let token = token().map_err(|error| error.to_string())?;
    let event_loop = EventLoopBuilder::<Wake>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let listener = proxy.clone();
    std::thread::Builder::new().name("widget-window-stdin".into()).spawn(move || {
        for line in std::io::stdin().lines() {
            let Ok(line) = line else { break };
            if let Some(message) = ToHelper::parse(&line)
                && listener.send_event(Wake::App(message)).is_err()
            {
                return;
            }
        }
        let _ = listener.send_event(Wake::Orphaned);
    })?;
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(tao::dpi::LogicalSize::new(SIZE.0, SIZE.1))
        .build(&event_loop)?;
    // Widgets keep their sign-ins and storage between runs, apart from the browser's.
    let data = rune::dirs::AppDirs::new(crate::APP_ID).map(|dirs| dirs.data().join("widgets"));
    let mut context = wry::WebContext::new(data);
    let (from_page, navigating) = (Rc::clone(&approved), Rc::clone(&approved));
    let proven = token.clone();
    // Each address handed to the browser once: a page can't open tab after tab by itself.
    let handed: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    let mut builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(url)
        .with_ipc_handler(move |request| {
            if origin(&request.uri().to_string()).as_deref() != Some(from_page.borrow().as_str()) {
                return;
            }
            if let Some(message) = accept(bridge, &proven, request.body()) {
                say(&FromHelper::Message { message });
            }
        })
        .with_navigation_handler(move |target| {
            let stays = navigable(&target, &navigating.borrow());
            if !stays && leaves_for_browser(&target) && handed.borrow_mut().insert(target.clone()) {
                say(&FromHelper::OpenUrl { url: target });
            }
            stays
        })
        .with_on_page_load_handler(move |event, _| {
            let _ = proxy.send_event(Wake::Loaded(matches!(event, PageLoadEvent::Finished)));
        })
        .with_permission_handler(|kind| match kind {
            PermissionKind::Microphone
            | PermissionKind::Camera
            | PermissionKind::DisplayCapture
            | PermissionKind::Geolocation => PermissionResponse::Deny,
            _ => PermissionResponse::Default,
        })
        .with_new_window_req_handler(|url, _| {
            say(&FromHelper::OpenUrl { url });
            NewWindowResponse::Deny
        });
    if let Some(shim) = shim(bridge, &token) {
        builder = builder.with_initialization_script_for_main_only(shim, true);
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(&window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().ok_or("the window has no box for its web view")?;
        builder.build_gtk(vbox)?
    };
    let mut loaded = false;
    let mut waiting: Vec<String> = Vec::new();
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Wake::Loaded(finished)) => {
                loaded = finished;
                say(if loaded { &FromHelper::Loaded } else { &FromHelper::Loading });
                if loaded {
                    flush(&webview, &mut waiting, &approved.borrow());
                }
            }
            Event::UserEvent(Wake::App(ToHelper::Post { message })) => {
                waiting.push(message);
                if loaded {
                    flush(&webview, &mut waiting, &approved.borrow());
                }
            }
            Event::UserEvent(Wake::App(ToHelper::Open { url, title, .. })) => match origin(&url) {
                // The page anew: the widget starts over, and what was meant for the page before
                // goes with it.
                Some(page) => {
                    window.set_title(&title);
                    loaded = false;
                    waiting.clear();
                    *approved.borrow_mut() = page;
                    // The same document (whatever its fragment) would only move within the page: it
                    // is reloaded instead, at the address asked for (its fragment may carry the
                    // widget's settings, and the page may have changed it since).
                    let same = webview.url().is_ok_and(|showing| same_document(&showing, &url));
                    let loading = if same {
                        webview.evaluate_script(&reload_script(&url)).or_else(|_| webview.reload())
                    } else {
                        webview.load_url(&url)
                    };
                    if loading.is_err() {
                        say(&FromHelper::Failed);
                        *flow = ControlFlow::Exit;
                    }
                }
                None => eprintln!("better-matrix: the widget's address has no origin"),
            },
            Event::UserEvent(Wake::App(ToHelper::Close) | Wake::Orphaned)
            | Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *flow = ControlFlow::Exit,
            Event::LoopDestroyed => {
                // Keeps the context, which the web view uses, until the window is gone.
                let _ = &context;
                say(&FromHelper::Closed);
            }
            _ => {}
        }
    })
}

/// Posts the waiting messages into the page, each a JSON object, if the document showing is on
/// the approved origin: the script runs inside that document, which could have replaced what it
/// calls, so a document of any other origin (one of its own making included) gets none of them.
/// They wait for a load that brings the approved page back.
fn flush(webview: &wry::WebView, waiting: &mut Vec<String>, approved: &str) {
    let showing = webview.url().unwrap_or_default();
    if !delivers_to(&showing, approved) {
        return;
    }
    for message in waiting.drain(..) {
        if let Some(script) = post_script(&message, approved)
            && let Err(error) = webview.evaluate_script(&script)
        {
            eprintln!("better-matrix: could not post into the widget: {error}");
        }
    }
}

/// Whether messages for the page go into the document at `showing`: only one on the approved
/// origin. `about:`, `blob:` and `data:` documents have none, so never.
pub fn delivers_to(showing: &str, approved: &str) -> bool {
    origin(showing).as_deref() == Some(approved)
}

/// An address's origin (`https://board.example.org:8443`), if it has one: `http` and `https`.
pub fn origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| parsed.origin().ascii_serialization())
}

/// Whether the page may go to `target` itself: on its approved origin, or a page of its own
/// making (`about:`, `blob:` and `data:` documents, which reach nothing). wry's handler can't
/// tell the main document from a frame (it sees frames' navigations too on Linux and macOS), so
/// such documents load in either; one shown as the main document gets no messages
/// ([`delivers_to`]).
pub fn navigable(target: &str, approved: &str) -> bool {
    match url::Url::parse(target) {
        Ok(parsed) if matches!(parsed.scheme(), "about" | "blob" | "data") => true,
        Ok(_) => origin(target).as_deref() == Some(approved),
        Err(_) => false,
    }
}

/// Whether an address the page may not go to opens in the browser instead.
fn leaves_for_browser(target: &str) -> bool {
    url::Url::parse(target).is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https" | "mailto"))
}

/// The script that posts `message` into the page as an object, as a parent window would, to a
/// document on `origin` (the approved one) only. It checks the document it runs in first (a
/// script sent during a navigation can run in the incoming document), so the message exists in
/// a page's reach only once that page is on `origin`, and addresses it there too. `None` unless
/// it is a JSON object. The message goes in as an object literal, which JSON is, and the origin
/// as a string literal, each escaped for a script (`<`, U+2028, U+2029): nothing in either runs.
pub fn post_script(message: &str, origin: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(message).ok()?;
    if !parsed.is_object() {
        return None;
    }
    let literal = script_literal(&parsed)?;
    let origin = script_literal(&Value::String(origin.to_owned()))?;
    Some(format!(
        "(function () {{ if (window.location.origin !== {origin}) return; var message = {literal}; \
         window.postMessage(message, {origin}); }})();"
    ))
}

/// `value` as JSON, which is a JavaScript literal, with what a script could still read otherwise
/// escaped: `<` (no `</script>`), and the line separators older engines end lines at.
fn script_literal(value: &Value) -> Option<String> {
    let json = serde_json::to_string(value).ok()?;
    Some(json.replace('<', "\\u003c").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
}

/// What the shim sent through the web view's IPC (`{"token", "message"}`), as the app gets it:
/// the message as JSON text, if the token is this window's and the bridge lets it out.
pub fn accept(bridge: Bridge, token: &str, sent: &str) -> Option<String> {
    let Ok(Value::Object(sent)) = serde_json::from_str::<Value>(sent) else { return None };
    if sent.get("token").and_then(Value::as_str) != Some(token) {
        return None;
    }
    let message = sent.get("message")?;
    forwards(bridge, message).then(|| message.to_string())
}

/// Whether the page's `message` goes out to the app, by the bridge: the widget API's requests
/// from the widget and its answers to the app's, or an integration manager's requests. Holding a
/// response means having the key, even when it is null, as the shim's `in` reads it.
pub fn forwards(bridge: Bridge, message: &Value) -> bool {
    let Value::Object(data) = message else { return false };
    let answers = data.contains_key("response");
    let holds = |field: &str| data.get(field).is_some_and(|value| !value.is_null());
    match bridge {
        Bridge::None => false,
        Bridge::WidgetApi => match data.get("api").and_then(Value::as_str) {
            Some("fromWidget") => !answers,
            Some("toWidget") => answers,
            _ => false,
        },
        Bridge::Integrations => holds("action") && !holds("api") && !answers,
    }
}

/// The script injected into the page's main frame before its own: what the page posts that
/// the bridge lets out ([`forwards`] says the same) goes to the app with `token`, and not into
/// the page; everything else is posted as before. None when nothing goes out.
pub fn shim(bridge: Bridge, token: &str) -> Option<String> {
    let test = match bridge {
        Bridge::None => return None,
        Bridge::WidgetApi => {
            "(data.api === \"fromWidget\" && !(\"response\" in data)) || (data.api === \"toWidget\" && \"response\" in data)"
        }
        Bridge::Integrations => "data.action != null && data.api == null && !(\"response\" in data)",
    };
    let token = serde_json::to_string(token).ok()?;
    Some(format!(
        "(function () {{\n\
         \x20 var ipc = window.ipc;\n\
         \x20 var token = {token};\n\
         \x20 var post = window.postMessage;\n\
         \x20 window.postMessage = function (data) {{\n\
         \x20   if (data !== null && typeof data === \"object\" && ({test})) {{\n\
         \x20     try {{ ipc.postMessage(JSON.stringify({{ token: token, message: data }})); }} catch (error) {{}}\n\
         \x20     return;\n\
         \x20   }}\n\
         \x20   return post.apply(window, arguments);\n\
         \x20 }};\n\
         }})();\n"
    ))
}

/// Whether two addresses name the same document: equal but for their fragments.
pub fn same_document(a: &str, b: &str) -> bool {
    let document = |address: &str| address.split('#').next().unwrap_or_default().to_owned();
    document(a) == document(b)
}

/// The script that reloads the page as a new document at `url`, the same document's address
/// with the fragment asked for.
pub fn reload_script(url: &str) -> String {
    let literal = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".to_owned()).replace('<', "\\u003c");
    // Reloaded even if the page broke the history API.
    format!("try {{ history.replaceState(history.state, \"\", {literal}); }} finally {{ location.reload(); }}")
}
