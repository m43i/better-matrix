# 0012. Widgets and integrations

Status: Accepted
Date: 2026-11-25

## Context

- Widgets are web apps pinned to a room as state (`im.vector.modular.widgets`, which Element
  writes, and the proposed `m.widget`): boards, dashboards, pads, and video meetings (Jitsi, BigBlueButton).
  The widget API (MSC2762) lets a widget page read and send the room's events through the
  client, within permissions (capabilities) the user grants. Integration managers (Element's
  `scalar.vector.im`, Dimension) are pages that add bots, bridges and widgets, talking to the
  client with Element's "scalar" messages.
- Rune has no web view and exposes no native window handle; on Wayland a native web view can't
  be parented into a non-GTK window. Drawing a browser engine offscreen into Rune would mean
  embedding Chromium (CEF): 150–250 MB per build, a helper and sandbox per platform, and
  Chromium's update cadence. Phones have system web views (`react-native-webview`).
- matrix-sdk ships a widget driver (`experimental-widgets`): it speaks MSC2762 over plain JSON
  strings and asks a `CapabilitiesProvider` what to grant.

## Decision

- **Listing:** `matrix_core::widgets` reads a room's widget state from the store and, under
  sliding sync (which leaves widgets out of its fixed `required_state`), from the room's state on
  the server, read once a run per room and shared with image packs (`Account::extra_state`,
  `EXTRA_STATE`); of two copies of an event the newer counts. The widgets panel asks afresh
  (`Command::ListWidgets`); the engine also reports a room's widgets (`Event::Widgets`) when its
  timeline opens and, read again from the server, whenever sync brings a widget state event
  (`follow_widgets`; under sliding sync the store keeps no state from timelines), so a room
  always knows its meetings. Adding, removing, `can_add` and `can_remove` are as before: an
  `m.custom` widget with a name (the host when blank), removal writes an empty event under every
  type the widget is pinned as, after a confirmation. A widget whose URL isn't `http(s)` isn't
  listed. URL template variables are filled for the user and room (`$matrix_user_id`,
  `$matrix_room_id`, `$matrix_display_name`, `$matrix_avatar_url`, `$matrix_widget_id`, the
  MSC2873 client ID, theme and language, the device ID, the homeserver, then the widget's own
  `data`), each URL-encoded as Element does.
- **Meetings** (`widgets::meetings`, `Widget::meeting`): Jitsi (`jitsi`, `m.jitsi`,
  `dimension-jitsi`; server and conference from `data`, else the URL's `conferenceDomain`,
  `domain`, `conferenceId`, `confId`, older widgets meaning `jitsi.riot.im`) and BigBlueButton
  (Dimension's `bigbluebutton` with `conferenceUrl`; a meeting Dimension created, an `m.custom`
  widget on its HTTPS `/widgets/bigbluebutton` page, joined through Dimension's `getJoinUrl`,
  which learns who the user is, so only after the Identity permission for Dimension's origin). A
  meeting opens as itself, never through Element's wrapper page (which needs Element):
  - Jitsi's page carries the user's name and audio-only in Jitsi's URL settings
    (`#userInfo.displayName="…"`, `config.startWithVideoMuted`, `config.disableDeepLinking`).
    A server that checks who joins (`auth: openidtoken-jwt`, Prosody's
    `mod_auth_matrix_user_verification`) gets `?jwt=`: HS256 over `{iss, sub: domain, aud,
    room: "*", context: {matrix: {token, room_id, server_name}, user}}` with an OpenID token, as
    Element's wrapper builds it (the server reads the token, not the signature), only once the
    user let that server learn who they are (the Identity permission, kept for it); refused, the
    meeting opens without one.
  - A pinned Microsoft Teams or Zoom HTTPS join link is a meeting. The engine recognizes the
    provider's meeting paths on its own domains, and opens the original join link in the system
    browser on every platform. These providers handle sign-in and app handoff. They are configured
    by adding an existing join link as a widget.
  - Starting one (`Command::StartMeeting`) writes a Jitsi widget as Element does (its wrapper
    URL and `data`, `name` "Jitsi", a 24-character ID) on the server the homeserver's well-known
    file names (`io.element.jitsi`, then `im.vector.riot.jitsi`, `preferredDomain`, looked up
    once per account, `Account::meeting_server`; a lookup that fails or isn't 2xx names none).
    Rooms offer it, and the engine starts one, only there, to whoever may pin widgets
    (`RoomSummary::can_start_meeting`): the room header's video button, `/meet`, the room menu
    and the widgets panel's +. The run then reads the new widget from the server, before sync
    brings it. A checking server (its `/.well-known/element/jitsi`, 2xx) gets the room ID in
    base32 as the conference, which the Prosody module checks. Element users see and join it.
  - The timeline shows widget state as room events (`RoomChange::WidgetAdded` with its meeting,
    `RoomChange::WidgetRemoved`): "Ada started a meeting".
- **Running a widget** (`Command::RunWidget`, `widgets::session`) is one long-running command:
  `browser` opens it in the browser (`Event::OpenUrl`, a meeting's fresh join link); else the
  app shows `Event::WidgetPage` and passes messages both ways (`Event::WidgetMessage`,
  `Answer::WidgetMessage`) until it cancels the run (`Command::Cancel`) or the account signs
  out (which ends its runs). `WidgetPage` again for the run means: load the page again. A run
  reads its widget from the server's resolved state, so one removed since sync last said doesn't
  open. The page's bridge says which messages pass (`WidgetBridge`): none for a meeting; the
  widget API's (`fromWidget` requests, `toWidget` responses); or the integration manager's (an
  `action`, no `api`, no `response`; a `response` key counts even when `null`). Only an HTTPS
  page gets a bridge: on HTTP whoever is on the way could stand in for it, so such a widget
  opens in the browser. A page in a web view has no parent, so it posts to itself: the URL gets
  `widgetId` and, as `parentUrl`, the page's own origin.
  - The bridge script runs in the page's main frame only, at document start (Android registers
    it with `addDocumentStartJavaScript` through the patch below, and phones inject it again at
    a load's end when a document reported nothing), with a random token for the run. It takes
    over `window.postMessage`: what the bridge lets out goes to the app with the token, never
    back into the page (Dimension would read its own request as the answer); the rest posts as
    usual. The app drops messages without the token (Android hands its bridge to every frame)
    or from another origin than the page's, and keeps a bridged page on its origin: another
    site opens in the browser; one that commits anyway counts as a new document.
  - Engine messages go in only while the page's current address (checked natively) is on the
    approved origin, through a script that itself checks `window.location.origin`, which no page
    can fake, before the message (an object literal) exists in the page, and posts it to that
    origin: a document that replaced the page, or an opaque one, gets nothing.
  - Each document reports its start and load (`Answer::WidgetLoading`, `Answer::WidgetLoaded`;
    desktop from the web view's page-load events, phones from the bridge script, so History API
    changes aren't loads). A start retires the driver and withdraws its questions; a load starts
    a new driver, and what the page posted before waits for it. The driver asks first unless
    the widget says it will announce itself (`waitForIframeLoad: false`, beside its URL or in
    its data); one that hasn't within 5 s is asked first, and if it was only slow (it never
    answered that), its own announcement starts the widget API over; if its answer comes after
    the driver's deadline, or to a driver since replaced, the page loads again (once a run),
    and the widget gets 20 s to announce itself. An
    announcement that comes before the answer waits 2 s for it, matched by its request ID
    (`Handshake`).
  - The SDK's driver drops an answer after ten seconds, and the widget API answers its first
    question only once per page: a permission answered later is kept and the page loaded again,
    whose new driver finds it.
- **Listing, kept fresh:** besides the timeline opening, widget state events and redactions in
  the rooms on screen read their widgets again from the server; a redaction elsewhere drops
  that room's cached state, and missed sync updates every room's, read again when it opens.
  Reads of a room's state are numbered: one overtaken by a later read, or by something
  forgotten meanwhile, gives way.
- **Opening a widget:** the engine resolves the room's current state, then asks for confirmation
  before loading a generic widget or handing its URL to a browser. The prompt identifies the
  origin, room and Matrix account, and names any Matrix context placed in the URL. Jitsi's
  confirmation discloses the display name it passes to the meeting. Widget API permissions and
  meeting identity access are requested separately.
- **Permissions:** what a widget asks for arrives as `Event::WidgetPermissions`, numbered within
  the run, each capability described (`WidgetPermission`, `PermissionKind`, its subject); the
  user starts with no options selected and grants any subset (`Answer::WidgetPermissions`, naming the question: an answer to another
  is dropped, and each prompt's controls are bound to the question they show). An empty
  question withdraws the one it names. Answers are kept per room, widget and the origin its
  page is on in the account's store (`Grants`): asking again for nothing new gets the earlier
  grant without asking; anything new, or the widget moved to another site, asks again. Learning
  who the user is (`get_openid`, which the SDK would answer on its own) is a permission of its
  own (`PermissionKind::Identity`, its subject the origin that would learn it, not the
  widget's room-set name); refused, the widget gets `blocked`. The widget's permissions action
  lists granted access for the current origin and can reset both grants and denials, closing its
  running page first. File downloads are described as
  what they are: files the account can open.
  - Camera and microphone: only a meeting's page, on the approved host (its own; iOS `prompt`,
    WebKit asking per site; Android `grantIfSameHostElseDeny` with the host in its own setting,
    `mediaCaptureHost`, checked natively when asked and again after the system's dialog, one
    dialog at a time); a move to another host asks the user first (Android). Other widgets are
    refused: desktop denies every request; phones deny natively. Android's web view is patched
    for this (`patches/react-native-webview@14.0.1.patch`): it ignored the grant type, granted
    whatever the app held, logged failing URLs, and injected the bridge late.
  - No widget gets the location: the desktop window denies its requests (WebKitGTK, WebView2;
    WKWebView offers no hook, and the app holds no location permission there); Android's web view
    has geolocation off, and every phone page's main frame has it fail (iOS frames inside a page
    still meet WebKit's per-site prompt). Phones log
    a failed load by its code only (its address may carry a token).
- **Integration manager** (`Command::OpenIntegrations`, `widgets::integrations`): the first
  valid HTTPS entry in the homeserver's well-known file (`m.integrations`, 2xx). When none is
  configured, the app offers Element's hosted manager. The origin, account and room are shown for
  confirmation before contacting either provider. An OpenID token
  registered at it (`/register?v=1.1`) gives a token, kept per account and manager and checked
  (`/account`) before use; a refused one is registered again. Unsigned terms
  (`M_TERMS_NOT_SIGNED`, MSC2140; at the manager's origin, as Element reads them) are shown
  (`Event::IntegrationTerms`) and agreed (`Answer::Agree`). Only an HTTPS manager is used: the
  tokens go there, requests carrying credentials follow no redirect (`credential_http`), and
  request errors leave out URLs. Its page's scalar requests are answered for the room it was
  opened for only: membership state, invite, bot options and power, member count, join rules,
  plumbing state, reading and setting room widgets (account widgets aren't supported), whether
  the user may send an event, encryption, sending integration state (`SENDABLE`: widgets, their
  layout, bot options, plumbing; never messages or the room's settings), and closing. Its
  replies are never taken as requests. Widget changes list the room's widgets again.
- **Desktop:** a widget runs in its own window from a web-view helper: the same binary started
  as `better-matrix --widget-window` (wry and tao; WebKitGTK on Linux, WKWebView on macOS,
  WebView2 on Windows), one process per run, JSON lines over stdin and stdout
  (`platform::widget_window`). It forwards the bridge's messages (token, origin and filter
  checked again in Rust), reports each document's start and load, posts the engine's after
  load, reloads on a repeated `open`, sends pages opening new windows and navigations off the
  page's origin to the browser (wry can't tell frames from the page there, so another site's
  frame opens in the browser too, and a top-level `data:`/`blob:` document may appear but gets
  nothing), reloads rather than moves within the page when asked for the same document again (at the
  address asked for, the page's own fragment changes undone),
  denies camera, microphone, screen and location requests, and exits with its stdin. Questions
  from runs wait in a queue (`State::widget_prompts`), the dialog only pointing at one: another
  dialog never loses one, and the oldest shows whenever no dialog does. Closing the window
  cancels the run; the run ending closes the window. Meetings open in the browser, which runs
  WebRTC reliably.
  - The room header's video button is the one meeting control: it runs the first of the video
    call (where calls work and none is under way), joining a pinned meeting and starting one
    (where offered), with a chevron listing the others (`VideoOption`,
    `Popover::VideoOptions`); with none it hides. Room info's **Integrations** row opens the panel.
  - The Integrations panel lists widgets and meetings, service accounts explicitly signaled in
    room state, and bridges with their advertised destination. Manage integrations has its own
    action. The + menu offers Start a meeting and Add widget where room rights allow them.
    Previously loaded integrations remain visible if refreshing fails, with a retry action.
  - `/meet` and the room menu start a meeting where offered. Permissions and terms are modals.
    Scenes: `demo-widgets`, `demo-widgets-menu`, `demo-meeting`, `demo-video-options`,
    `demo-meeting-no-calls`, `demo-meeting-start`, `demo-meet-command`,
    `demo-widget-permissions`, `demo-integration-terms`.
- **Phones** run every widget, meetings included, in an in-app web view (`app/widget.tsx`,
  `WidgetRun`); see [0010](0010-mobile.md).

## Consequences

- On Linux the app links WebKitGTK 4.1, GTK 3, libsoup 3 and JavaScriptCore, so they are runtime
  dependencies of every Linux package, and the AppImage leaves them to the system (WebKit's
  helper processes can't run from a bundle). Windows needs the WebView2 runtime (part of
  Windows 11 and updated Windows 10).
- The desktop widget window gives pages no camera or microphone; meetings open in the browser.
- The Android web view carries a patch (`patchedDependencies`); bumping `react-native-webview`
  means porting it, and checking the patched module still compiles.
- Interoperability rests on Element's and Dimension's conventions (widget content, the Jitsi
  JWT, scalar messages, `register?v=1.1`), partly unspecced. Integration managers are being
  retired by some deployments; the app follows the homeserver's well-known file.
- The widget window can't be tested headless: tests cover the helper protocol, the bridge
  filters, state and views; a real window was checked by hand against a local page.
