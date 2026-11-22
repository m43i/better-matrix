# 0010. Mobile

Status: Accepted
Date: 2026-11-16

## Context

- The app comes to Android and iOS with every feature the desktop has, in the desktop's design,
  laid out the way people expect a chat app on a phone to work (Slack's mobile app is the model:
  tabs at the bottom, a room list, a native stack into a conversation, sheets for actions).
- All Matrix logic already lives in `matrix-core` behind the `Command`/`Event` boundary
  ([0002](0002-engine-and-desktop-state.md)), which was kept free of SDK types for this.
- Rune draws its own widgets; on phones the platform's own navigation, sheets, pickers, menus,
  text input and accessibility are what users expect and what stores review against.
- Development happens on Linux: Android builds and runs here, iOS needs a Mac.

## Decision

- **React Native with Expo** (`apps/mobile`, Expo SDK 58, React Native 0.88, the New
  Architecture). Expo Router gives a native stack (react-native-screens), native tabs
  (`expo-router/native-tabs`: `UITabBar`, Material's bottom navigation), native headers and
  sheets; Expo's modules give
  pickers, the camera roll, audio, video, notifications, the keychain and secure browser
  sessions. Native folders are generated (`expo prebuild`) and not committed; native settings go
  in `app.json` and config plugins.
- **Navigation:** the platform's tab bar holds Home and Direct messages. Each tab is a folder
  with its own native stack (`ui/shell/TabStack`) for its header; everything opened from a tab
  (rooms, sheets, modals) is declared on the root stack and covers the tabs.
  - _Adding an account:_ sign-in opens a full-screen page with an X at the top left to cancel
    and return; it shares the first-account sign-in form (ADR 0003).
  - _Header:_ the account on screen is its picture at the header's right (`ui/shell/TabHeader`;
    on iOS a `Stack.Toolbar.View` without the shared glass background; on Android one row with
    the title, since the native stack lays a custom title across the whole bar and a right item
    lands at its start), on every phone, opening the
    **account menu** sheet (switch or add an account, settings, sign out); a dot on it says
    another account has something new. Home's title is the scope with a chevron, opening the
    **space picker** (the desktop's rail). There is no notification list: the system shows
    notifications, and the header has no bell.
  - _Search:_ under each tab's title, as the desktop's top bar: rooms, direct chats and spaces
    by name, then people from the user directory, listed in the tab's place while something is
    typed (`ui/shell/SearchResults`, from the pure `searchResults`), and the tab's list behind it
    otherwise. Messages stored on this device form the last section (`SearchMessages`, ADR
    0008); names and message excerpts highlight fuzzy matches. Per-account `SearchUsage` from
    the engine ranks frequently used rooms, people and messages within each section. A result opens at its event,
    including a reply inside its thread. A pasted invitation link or a typed address (`#room:server`, `@user:server`,
    `!room`) is offered first as a link to open, and the search key opens it at once
    (`parseTyped`, [0008](0008-people-and-rooms.md)). iOS uses the header's own search bar,
    stacked under the title (`placement="stacked"`); Android has no header search bar there, so
    a Material-style field sits under the header at the top of the tab
    (`ui/shell/ShellSearch.android.tsx`).
  - _Create:_ the desktop's create menu (a chat, a room where the scope allows, a space from
    Home, a section; `state/create.ts`) lives at the bottom. Android shows Material 3's floating
    action button menu at each tab's bottom right (`ui/shell/CreateFab.android.tsx`): labelled
    buttons above the button, which turns into a close button over a scrim. iOS puts "+" in the
    tab bar, `disabled` (the bar never selects it but still reports the press) and opening the
    create sheet (`app/create.tsx`): iOS 27 sets it on its own surface (`prominent`), iOS 26.1
    in the search tab's separated place (`role="search"` with its search never activated, the
    only way to set a tab apart there), and older iOS shows it as a third tab
    (`ui/shell/create.ts`'s `createTab`).
  - Header buttons and menus elsewhere are `Stack.Toolbar` items; Android needs their icons as
    images, so the Material Symbol is drawn to one (`ui/shell/toolbarIcon`). Room lists are as
    compact as the desktop's sidebar (`#` for rooms, a small avatar for direct chats); the DMs
    tab adds the latest message. Every avatar is the desktop's rounded square. Dragging rooms
    between sections becomes the room's long-press sheet, which edits the same `SidebarLayout`.
- **The engine is `matrix-core`, unchanged in shape.** `packages/matrix-ffi` wraps it in one
  UniFFI object, `Bridge`; `uniffi-bindgen-react-native` (ubrn) builds it for Android
  (`cargo ndk`, arm64 and x86_64) and iOS and generates a JSI turbo module
  (`packages/matrix-native`). Calls are synchronous JSI; nothing goes through the old bridge.
- **Commands and events cross as JSON.** `matrix-core`'s `ffi` feature derives serde and ts-rs on
  every boundary type: data enums are `{ type, data }`, unit enums strings, field names as in
  Rust. The TypeScript types are generated from the Rust (`bun run bindings:types`) into
  `packages/matrix-native/src/generated/types`; a test fails when they drift. The FFI surface
  stays three calls (`start`, `send`, and the listener), so a new feature adds no FFI code; the
  one exception is a push handled without the engine (`fetch_push`, Push below).
- **External JavaScript data is decoded at its boundary.** Zod validates saved settings and
  pusher registrations, Expo configuration, notification payloads, and the push gateway's Matrix
  requests and Expo ticket responses. Downstream functions receive the decoded domain types.
  Rust bridge JSON is trusted from the shared generated types; assertions there document that
  invariant. Tests inject call permissions, media resolution, and navigation through their
  interfaces instead of replacing modules.
- **Bytes become files.** `MediaLoaded` and `GifLoaded` are written under the cache directory,
  named by what they are, and reported as `MediaFile` / `GifFile` with the path; image and
  video views open the file directly. Uploads go the other way as paths (`Upload::Path`);
  `Upload::Bytes` doesn't cross.
- **Events leave on one thread** (`bridge-events`) in the engine's order; calling JavaScript
  from it blocks until JavaScript has handled the call, so the engine's threads never wait on
  the UI. Each call is a JavaScript task of its own (React renders between tasks), so the thread
  sends what queued up meanwhile as one JSON array (`events::next_batch`, at most 256 events or
  about 1 MiB), and a lone event as its object without waiting. `Engine.emitJson` takes either;
  the store takes a batch in as one update.
- **Secrets** go to the iOS keychain and Android's keystore through `expo-secure-store`
  (available after first unlock), called back synchronously from the engine's blocking threads.
- **OAuth on phones returns to the app.** `EngineConfig::oauth_redirect` is `Loopback` on the
  desktop and `App(uri)` on phones: the browser session (`ASWebAuthenticationSession`, Custom
  Tabs) returns to the private-use scheme `org.example.bettermatrix:/oauth` (the client URI's
  host reversed, as MSC2966 requires) and the app hands the URL back with
  `Command::FinishOAuth`. The scheme changes with `CLIENT_URI` once the project has a site.
- **One store, one reducer, as on the desktop.** `apps/mobile/src/state`: `apply(state, event)`
  is the only way engine data enters the app; it is pure and tested by feeding events. User
  actions are functions in `state/actions/*` that send commands. `Store.send` fires and forgets
  (a failure becomes a notice unless `quiet`); `Store.run` awaits a request's `Finished` and
  hears the events answering it (a discovered homeserver, a URL to open, people found), which
  is the mobile form of the desktop's `Purpose`. Views read through `useStore(selector)`;
  notifications to React are batched per burst of events. Events that repeat what the store
  holds keep its objects (a re-sent room list, unchanged call levels), so memoised views don't
  redraw. A closed timeline keeps its entries while the engine keeps it warm (eight per
  account); after that, a cold timeline, only its newest page, since opening it again starts
  from the engine's newest anyway, and once no timeline of its room is open or warm the room's
  members, threads, files and pins go too (`forgetRoomDetails`): every screen showing them
  (room info, readers, invite, a member, pinned messages) is pushed over the room and asks for
  them when it mounts. Selectors run on every notification, so they stay cheap: what they derive
  from a list is kept per list (the scope helpers in `state/sections.ts`, the badge), the room
  lists select only the fields they draw (`activeRoomList`), of the timelines only each room's
  newest message as the list shows it (`newestMessages`, the same object until one changes), and
  what every row needs is read once and handed down by context (the palette from
  `PaletteProvider` at the root, a timeline's pins).
- **Media, link cards and GIFs are wanted by the views showing them** (`Store.want`, released on
  unmount). Each is fetched once a run; one that failed (the fetch, or writing its file) waits as
  the desktop's do (`state/retry.ts`, the desktop's `Retries`: 5 s, doubling, up to 5 minutes;
  the 512 failures to be tried again last are remembered), and the store fetches it again when
  the wait is over if a view still wants it, else the next view that does. Views keep no timers
  of their own, so a list of pictures from a server that is down costs one fetch per picture per
  wait.
- **The engine hears events in batches** (`Engine.subscribeBatches`): each listener hears a
  whole event batch before the next listener hears any of it. The store is the only listener in
  the app (the demo engine adds one), so nothing depends on listeners' interleaving.
- **Design:** the desktop's palette roles (`ui/theme.rs`) as sRGB in `src/theme/palette.ts`,
  its type scale and identity colours; icons are SF Symbols on iOS and Material Symbols on
  Android through one name table (`src/ui/icons.tsx`) mirroring the desktop's icon set.
- **Each platform's own conventions,** decided in the shared components so screens don't branch:
  - _Headers:_ iOS tints buttons and the back chevron with the accent; Android draws the
    Material back arrow and actions in the text colour (`headerTint`), and titles sit at the
    start. A modal leaves by Cancel (or Done, at the right) on iOS and by a close icon at the left
    on Android (`ui/HeaderButtons`).
  - _Lists:_ iOS's inset grouped sections (an upper-case caption over a rounded card), a
    disclosure chevron on rows that open something, a checkmark on the chosen row; Android's
    flat Material sections (a caption in the accent), no chevrons, radio buttons
    (`ListSection`, `ListRow`).
  - _Switches:_ UISwitch on iOS, Material 3's Compose switch on Android (`ui/Toggle`), since
    React Native's Android switch is Material 2's.
- **Translations:** the desktop's catalogs are reused as they are, with a mobile catalog per
  area of the app layered on top (`src/i18n/mobile/<area>.<locale>.json`); a test fails when
  German or French lacks a mobile key or a placeholder.
- **The composer is one row:** + then the text field then the mic (send once there is something
  to send). The field grows with its text to about six lines, then scrolls, as the desktop's
  does. + opens every other option in the keyboard's place (photos and videos, camera, files,
  poll, formatting, emoji); formatting, once picked, puts the toolbar above the row until it is
  closed.
- **The composer writes formatting as markers.** The native text field holds plain text;
  `Format`s insert and remove markdown-style markers (`**bold**`, `_italic_`, `~~strike~~`,
  `` `code` ``, `[text](url)`, `- `/`1. ` lists, `> ` quotes, ``` fences), and lists and quotes
  continue on a line break. On send, `state/format.ts` turns the text into the body and the
  Matrix HTML the desktop's rich field sends (as Rune's `to_html` writes it), HTML only when
  formatted; `draftOf` writes a message's HTML back as markers for an edit. Sends are planned as
  data (`state/outgoing.ts`): text with the reply first, then each attachment as
  `Upload::Path`; voice memos are AAC `.m4a` from expo-audio, sent with a 100-point waveform
  from the recorder's metering (read 20 times a second, `waveformFromLevels`, on the desktop's
  scale).
- **Voice memos** look and behave as the desktop's voice card (ADR 0007), with the same pure
  arithmetic in `state/voiceCard.ts`. Recording, a blurred glow (`boxShadow`) surrounds the
  composer's field; its brighter halo's opacity is a Reanimated shared value the metering sets,
  so nothing re-renders for it, and it holds half lit under reduced motion (`VoiceGlow`). In
  the timeline, `AudioCard` makes its player only once the clip is wanted (play, a seek), so
  memos scrolled past make none; it shows play beside the waveform bars, a dot to drag (the
  bars' own responder; only letting go seeks, so the list taking the gesture leaves the clip
  where it was), the time, and once loaded the speed button (`settings.playbackSpeed`: 1×, 1.5×, 2×,
  pitch kept by expo-audio). One clip plays at a time (`audioFocus.ts`). A clip that plays to its
  end pauses and rewinds (Android's player would otherwise play again at the next seek); one
  moved to its very end waits there. expo-audio reports a paused clip's place only while it
  plays, so the card keeps where a seek left it.
- **Calls** keep the desktop's shape: the same commands and events, folded by the reducer;
  `ui/call` (header buttons, ring cards over every screen, the call bar on the tab screens) and
  the stage as a full-screen modal (`app/call.tsx`) that opens by itself when a call starts or is
  answered and closes when it ends. Pure parts (what the stage says per phase, tiles, controls,
  notices) live in `ui/call/stage.ts`; actions in `state/actions/calls.ts` ask for
  the microphone (and camera) first and send at once when they're already allowed. Rings buzz
  and loop the desktop's chime (`assets/sounds/ring.wav`, rendered from its `chime`) unless
  rings are silent.
- **Call layouts are the desktop's, ported as pure TypeScript** (ADR 0009):
  - _Planner:_ `state/callLayout.ts` is `state::call_layout` line for line (Auto, Grid, Focus;
    tile identity by participant key and surface kind; equal 16:9 cells paging before 160×90 pt,
    25 a page; the one-to-one self-view inset; the participant strip beside the focused surface
    when the stage is at least 640 pt wide and 1.5 times as wide as tall, else below, 18–40%),
    with its tests ported one for one.
  - _The call's view:_ `state/callView.ts` is the rest of `state::call` — `CallView` (layout,
    page, self-view, Full video / Fill tile per camera, strip, chat), kept in
    `State.call.view` beside the engine's `CallInfo`. The reducer starts a new call from the
    default view and reconciles the same call's view on every `CallUpdated` (a departed
    participant or ended pinned share returns to Auto); the actions (`setCallLayout`,
    `pinCallTile`, `toggleCallSelf`, `toggleCallFit`, `pageCall`, `setCallStrip`,
    `showCallChat`) are pure view changes. Speaker focus is the desktop's state machine
    (`hearCall`: loudest unmuted remote for 1.5 s, the 400 ms speaking hold), followed from the
    store's levels by one follower per store (`ui/call/useCallAudio.ts`), so going between the
    stage and the chat keeps the speaker, and a stage redraws only when the rings or the focus
    change.
  - _Watching:_ only planned tiles mount, and a `CallVideoView` watches its video while mounted
    (below), so the plan alone decides what is watched: hidden pages, a hidden self-view and
    a strip's other pages convert no frames, and hiding self-view never stops the camera.
    Going between the stage and the chat remounts the views, so a video may be unwatched and
    watched again across the transition.
  - _Stage_ (`app/call.tsx`, `ui/call/CallTiles.tsx`): the native header carries the room's
    name, the way back and the call layout menu (`Stack.Toolbar.Menu`: Auto, Grid and Focus
    for a video call with the current one checked, then Hide / Show self-view). Tapping a video
    tile pins it in Focus; cameras big enough carry the desktop's Full video / Fill tile button;
    swiping or the paging row turns pages; the strip's divider drags (or adjusts by screen
    reader) within 18–40%. The controls are the phone's (microphone, camera, switch camera,
    loudspeaker) plus Show chat and hang up.
  - _Source aspects are unknown on phones:_ the engine shapes each frame to its view in Rust
    and reports no source size to JavaScript, so every surface plans at 16:9 and cameras
    default to Fill tile (the desktop measures `Pixels` and shows portrait cameras whole).
    The planner already takes aspects (`Aspects`, by `VideoId`); reporting them needs an event
    from `matrix-ffi`'s video sink.
- **Chat during a call is the call's room with a compact stage on top** (the phone's form of
  the desktop's chat below a compact stage; one surface at a time on a phone). Show chat on the
  stage sets `view.chat` and `router.dismissTo`s the call's room: back to it when it's under
  the stage, else in place of the stage. While `view.chat` is set, that room's screen renders
  `ui/call/CompactCall.tsx` above its timeline: one surface at a time for a video call (the
  desktop's compact plan: the main surface first, then every other participant and screen,
  by swipe or arrows), a row of faces for a voice call, and small controls (microphone,
  camera, back to the call, hang up). Tapping it, the back control, the header's Return to
  call or the call bar open the full stage and clear `view.chat` (`useOpenStage`), so the
  stage and the chat never show at once. The compact stage needs no route of its own, the
  room's composer and keyboard handling stay as they are, and the choice lasts for the call
  like the desktop's display preferences.
- **Call media is native and never crosses JavaScript.** A local Expo module
  (`apps/mobile/modules/call-media`, Kotlin and Swift) does what the engine can't on phones:
  - _Android start-up:_ libwebrtc needs the Java VM and the application context before its
    audio device module exists. The module's `CallMediaNative` loads `libmatrix_ffi.so` with
    `System.loadLibrary` (the turbo module only links it, which doesn't run `JNI_OnLoad`), so
    `JNI_OnLoad` (`matrix-ffi/src/android.rs`) hands libwebrtc the VM, and
    `CallMediaNative.initialize(context)` runs `initialize_android_context` at module creation.
    libwebrtc's Java half (`livekit.org.webrtc.*`, `libwebrtc.jar` from the prebuilt download)
    ships in the app: `bun run ubrn:android` copies it to `packages/matrix-native/android/libs`
    and the module compiles against it. Its native methods are found by name, so
    `matrix-ffi/build.rs` keeps and exports libwebrtc's `Java_livekit_org_webrtc_*` symbols
    (`webrtc-sys` asks for that in its own build script, but a dependency's link arguments
    never reach the final library).
  - _Video views:_ `CallVideoView` (a `TextureView` on Android, a layer on iOS) registers with
    the bridge for one `VideoId` (`matrix-ffi/src/video.rs`): the bridge keeps the engine's
    handle and sends `WatchVideo` with a sink that draws every frame into each view showing that
    video, cropped (cameras) or letterboxed (screens) to the view's shape in Rust; the last view
    to go sends `UnwatchVideo`. Android draws into the `Surface` through `ANativeWindow`; iOS
    gets a C callback (`bm_video_*`, `native.rs`). Views are keyed by call and video, since
    each call numbers its videos afresh.
  - _Camera:_ the app registers its camera with the engine once (`registerNativeCamera`, a
    UniFFI foreign trait the JavaScript side forwards to the module): the engine starts, stops
    and switches it as it would `nokhwa` (a platform camera, ADR 0009), and the camera's frames
    go to the engine natively (Android: libwebrtc's own Camera2 capturer, I420 over JNI; iOS:
    AVFoundation NV12 over `bm_camera_frame_nv12`), rotated upright in Rust. Cameras are named
    `front`, `back`, … and the stage's switch button picks the next.
  - _Audio route:_ communication mode while a call runs (`AudioManager` on Android, the
    `voiceChat` session on iOS), the earpiece for voice calls and the loudspeaker for video
    calls, switchable on the stage.
    Without the module (Expo Go, tests) calls still work as voice: the stage shows faces and hides
    the camera and speaker controls.
- **Screen sharing** is desktop-only: libwebrtc has no desktop capturer on phones, so
  `start_screen` reports it unavailable there.
- **TLS uses ring on phones.** `Engine::start` installs rustls' `ring` provider on Android and
  iOS (aws-lc on desktops): aws-lc's jitter-entropy self-test crashes on Android, where its
  weak sized-free hook resolves to non-code. Both reqwest versions in the tree take the
  process default provider.
- **Android trusts Mozilla's roots.** reqwest's platform verifier needs a Kotlin component (an
  AAR from the verifier's own Maven repository) and JNI set-up; until the app ships those, every
  HTTP client on Android (`src/tls.rs` in matrix-core: the account clients, discovery, links)
  trusts `webpki-root-certs` instead. User-installed CAs and the system's revocation checks are
  not honoured there yet. Desktops and iOS keep the platform verifier.
- **The OAuth return** is caught before the router: `src/app/+native-intent.ts` keeps the
  redirect URI from being routed, and because Android's Custom Tabs can report "dismissed"
  before the redirect arrives, the sign-in waits 1.5 s for it before treating the browser as
  cancelled.
- **Permalinks** ([0008](0008-people-and-rooms.md)) reach the app three ways: the `matrix`
  scheme on both platforms (`app.json`'s `scheme`), `https://matrix.to` on Android through an
  intent filter (`android.intentFilters`; unverified, as matrix.to is not ours: Android 12 and
  later open it in the app only once the user allows the app's supported links in its settings,
  or picks it in the chooser), and links in messages. iOS can't claim matrix.to: universal links
  need the domain's `apple-app-site-association` to name the app, so there matrix.to links open
  Safari (which offers the app through matrix.to's own page) and only `matrix:` links open the app.
  `+native-intent.ts` keeps a permalink from the router (`initial` → `/`, else `null`) and
  leaves it in the link inbox (`ui/links/inbox.ts`); `LinkOpener` opens it once an account's
  room list arrived and the router is mounted, so a link that started the app, or came while
  signed out, waits. Opening is synchronous and pure (`state/linkRoute.ts`, with the TypeScript
  parser `state/permalinks.ts` that runs the engine's vectors): a joined room by ID pushes the
  room at the linked message (`routes.room(…, eventId)`, switching the account on screen), and
  anything else pushes the link screen (`app/link.tsx`, a fitted sheet), which previews and joins
  through `state/actions/links.ts`. Message links (`openLink`), mention pills of rooms the account
  hasn't joined, and `matrix:` anchors (now kept by `state/html.ts`) go the same way, for the
  timeline's account; a matrix.to anchor to a message stays a link rather than a pill, so it
  opens at the message.
- **Timelines open while their screen is mounted,** counted per timeline
  (`retainTimeline`/`releaseTimeline`), not on focus: sheets over a room (message actions, the
  emoji picker) blur it, and reacting, redacting and voting need its timeline open. Focus still
  decides what is on screen (`state/focus.ts`): read markers and quiet notifications.
- **A tapped notification** opens its room, or the invitation's sheet when it is about a
  pending invitation (`tapRoute`), once the account's lists know the room as joined or invited:
  the cached list from before the first sync may lack what came while the app was closed, so a
  tap waits for sync, up to 15 s, then opens the room.
- **Developer tools and widgets** follow the desktop (ADR 0006, ADR 0008) in the platform's
  shapes: "Show hidden events" is a switch in Preferences (`Settings::showHiddenEvents`), and
  the timeline plan gives each hidden event a line that opens its source; "View source" in a
  message's sheet opens `app/event-source.tsx` (selectable JSON, Copy JSON). A room's details
  lead to `app/widgets.tsx`, which opens a widget in the browser (`Linking.openURL`) and, for
  whoever may change them, offers Remove (with an alert) and a + in the header to
  `app/add-widget.tsx`. No widget is embedded: no web view.
- **Message HTML** is parsed by a small tokenizer of our own (`state/html.ts`) into a safe node
  tree: only the desktop's allowed tags become formatting, links keep `http(s)`, `mailto` and
  `ftp`, reply fallbacks and scripts are dropped, nesting is capped. Nothing renders raw HTML.
- **QR codes** (ADR 0004): one scanner (`ui/qr/QrScanner.tsx`, `expo-camera`) serves
  verification, signing in with a QR code and linking a device. Matrix's codes are binary and
  the camera's barcode result is lossy text, so a sighted QR code (or a 2.5 s timer) only
  triggers a small picture, which `Command::ReadQrCode` reads in Rust. `QrCode` draws a
  `QrImage` from Views, black on white with a quiet zone. Sign-in and linking share one panel
  (`QrLoginPanel`) driven by `QrLoginStep`s through `Store.run`. Demo scenes show a placeholder
  instead of the camera that reads a demo code when tapped, and the demo engine holds QR sign-in,
  linking and registration until they are answered or cancelled.
- **Demo scenes** (`src/demo`) are the mobile `--scene`: the desktop's fixtures as engine
  events through `apply`, on a demo engine that finishes every command and answers lists,
  searches, sends and calls from the fixtures (media from the app's own assets). Dev builds
  switch to one with the link `bettermatrix://demo/<scene>`, which `+native-intent.ts` keeps from
  the router; release builds never read it. A test boots every scene. The call scenes mirror the
  desktop's: `call-voice`, `call-video`, `call-gallery`, `call-many`, `call-direct`,
  `call-shares`, `call-focus`, `call-chat`, `call-voice-chat`, `call-bar`, `ringing`; the
  layout menu is native and can't be opened by a scene. `preview-gallery` and `preview-audio`
  open a batch Grace posts into #design (two images, a video the demo has no file for, an
  audio clip).
- **The attachment preview** (`app/preview.tsx`) shows one attachment full screen on black: an
  image to pinch, pan and double-tap to zoom, a video in expo-video's player with the
  platform's controls, or an audio clip in a player of ours (`ui/preview/AudioPreview.tsx`:
  large play, a scrubber, time played and length, the timeline's speed setting; one clip at a
  time through `audioFocus.ts`; its file comes through the card's `fetchOriginal`, so a failed
  download says why, with Try again). Images and videos open it from the timeline; an audio card
  plays in place from its play button, and a tap elsewhere on it opens it in the preview.
  - _Gallery of a batch:_ Matrix carries one attachment per message, so a batch is a run
    (`state/gallery.ts`, the desktop's rule): from the opened message, backwards and forwards
    while the neighbour is an attachment from the same sender, the later of each pair within
    5 minutes of the earlier and not a reply. Read markers and entries the timeline doesn't
    draw don't break it; a date divider, a room event, any other message or a thread's root
    (its replies start their own run under it) does. Of the run the
    previewable items stay (images with a picture, videos and audio with a file). Two or more
    get a strip of thumbnails along the bottom (`ui/preview/GalleryStrip.tsx`, the timeline's
    preview sizes, so they are usually cached; the current one ringed in the accent and
    scrolled into view); a sideways swipe steps through them, round past either end
    (`ui/preview/swipe.ts`; not while an image is zoomed in, and not over a video's controls),
    and tapping a thumbnail shows it. The route keeps its one event ID; the item on screen is
    the screen's own state, and the batch is the one around it (`galleryAround`), so an item
    redacted meanwhile, the opened one included, leaves the rest; the top bar's sender, time and
    actions follow it. Each item's content is keyed by its message, so moving on releases the
    video or clip before it.
- **File actions:** message sheets offer Copy image only for images with media, plus Open,
  Share and Download; they never offer Copy message for an attachment filename or empty text.
  Attachment surfaces forward long presses to the message sheet, including image/video
  previews and audio controls. The full-screen preview opens a file-only fitted sheet from
  More or a long press on an image or audio clip (`attachment-actions`), preserving the
  preview when dismissed.
  `attachmentActions` supplies both sheets’ file actions; `AttachmentActions` owns their busy
  state and suppresses repeated actions until completion. Copy image fetches the original and
  passes its base64 bytes to `expo-clipboard`. Open uses `File.preview`: Quick Look on iOS,
  `ACTION_VIEW` on Android. Failure keeps the sheet available for retry and shows a notice.
- **Files leave through the system:** sharing or saving an attachment fetches the original,
  copies it into the cache under its own name and opens the share sheet (Android can also pick a
  folder to download to).
- **Push goes through Expo's push service and our own gateway** (`docs/push.md` is the release
  checklist):
  - _Why Expo:_ the FCM and APNs credentials live in EAS (`eas credentials`), not in the repo or
    on a server of ours, and both platforms get one kind of token (`getExpoPushTokenAsync` with
    the EAS project ID from the app config). Homeservers only speak the Matrix push gateway API,
    so `packages/push-gateway` (a dependency-free handler for Bun or a Cloudflare Worker)
    translates `POST /_matrix/push/v1/notify` into Expo's send API and answers `rejected` for
    tokens Expo reports as `DeviceNotRegistered`. It keeps and logs nothing.
  - _No content leaves the homeserver:_ pushers ask for `format: "event_id_only"`, so the
    gateway, Expo, Google and Apple see an account, a room and event ID, the unread count and
    the timing, never a message. The phone fetches and decrypts the event itself.
  - _One token, several accounts:_ each account's pusher (app ID `chat.bettermatrix.mobile`,
    registered with `append`, so accounts on one server keep each other's) carries
    `default_payload.account`; the gateway sends it with the room, event and count as the push's
    data, so the device knows whose push it is. iOS pushers add a fallback `alert` (in the
    user's language), which the gateway sends as a mutable alert; Android's are data only.
  - _Engine:_ `Command::RegisterPusher` / `RemovePusher` (matrix-sdk's pusher API) and
    `Command::FetchNotification`, answered by `Event::PushContent` (`Message`, `Invite`,
    `Hidden` for what can't be read, `Nothing` for what doesn't notify), fetched with
    matrix-sdk-ui's `NotificationClient` (sliding sync, else `/context`, decrypting with the
    account's store; `MultipleProcesses`, since the engine's own sync runs beside it).
  - _Without the engine_ (`matrix_core::push::fetch_without_engine`): one account's store
    opened from the data directory and the secret store, the event fetched, the store closed,
    within 22 s. Its clients hold the SDK's cross-process locks as `push`, the engine's as
    `main`: OAuth refreshes go through the cross-process refresh lock and are saved inside it
    (session callbacks on every client, which read the tokens back when the other process
    refreshed), and those callbacks are the only place tokens are saved. A store directory is
    found under the data directory wherever the account list says it was, so the data directory
    may move.
  - _The crypto store's lock:_ the engine holds it through each sync
    (`Account::sync_once_locked`), which reloads what a push process wrote and keeps the two
    from writing the store at once. Only the phones share (`EngineConfig::stores`:
    `StoreSharing::WithPush`); the desktop, where nothing else opens a store, is `Alone` and
    takes no cross-process locks. A held lock's lease is written to the store every 50 ms, so a
    phone syncing pays 20 writes a second per account. Syncing without it while the app is in
    front was tried and dropped: cancelling `Client::sync_once` when the app leaves the front can
    lose a response the SDK already counted as processed, the iOS marker is not an atomic
    handshake, and the engine's own push fetch (`MultipleProcesses`) writes the store beside the
    sync. Lowering this cost needs the lock held only while a response is processed. With
    sliding sync (ADR 0002) the lock covers the room-less long-poll sync, which carries every
    to-device message and key; the room list's sliding sync runs outside it, as the SDK
    designs `RoomListService` (and Element X runs it beside its push process). Decrypting room
    events there can still write a Megolm session's sender data, as an open timeline's
    decryption always could: a push process improving the same session at that moment could
    lose its improvement.
  - _Android:_ the push is an FCM data message (through Expo); expo-notifications runs the push
    task (`engine/pushTask.ts`, defined by the entry `index.ts` before the router, headless when
    the app isn't running). With the engine running it asks the engine; headless it calls
    `fetch_push` (UniFFI, a thread of its own, the app's secret store). In front it does
    nothing: the running engine notifies. Each event shows once per JavaScript context
    (`Seen`), whichever of sync and push comes first.
  - _iOS:_ a Notification Service Extension (`BetterMatrixPush`, Swift, added with its
    entitlements by `plugins/withPush.js`) links the engine's static library and calls
    `bm_push_fetch` (C, `matrix-ffi/src/push.rs`) with the payload, the data directory and its
    keychain functions; Rust parses the push, opens the store, fetches, and words the
    notification from `push-strings.json` (the app's catalog texts). The engine's data lives in
    the App Group container (launch moves it there once) and the secrets in a keychain access
    group both targets share (the app's default group, so nothing moves). While the app is in
    front (`app-active`, a timestamp it writes every 30 s) the extension leaves pushes alone and
    the app keeps them from showing; in the background the app leaves notifications of accounts
    with a pusher to the extension, since the two processes can't agree on what showed.
  - _App:_ pushers follow the accounts, the push token and the settings (_Notifications on this
    device_, and per account when there are several) in `engine/push.ts`, deciding in
    `engine/pushRules.ts`; what this device registered is kept in `pushers.json`, and signing
    out removes the account's pusher first. A push token that can't be had (offline at launch)
    is asked for again from 30 s, doubling to 10 minutes, while an account needs it
    (`tokenFetcher`). A step that fails is tried again a minute later (`carryOut`): a failed
    registration, or a failed removal, which stays in `pushers.json` and keeps a new token from
    replacing it until it goes; meanwhile Android drops pushes for accounts push is off for
    (`pushAllowed`). The gateway answers 502 when Expo asks to slow down
    (`MessageRateExceeded`), so the homeserver sends again. Without a gateway URL
    (`EXPO_PUBLIC_PUSH_GATEWAY`) or an EAS project, push is off and nothing else changes. The
    app icon's badge counts the conversations wanting attention while the app runs.

## Consequences

- One engine for every app: a fix in `matrix-core` reaches desktop and mobile at once, and a new
  `Command` or `Event` reaches TypeScript by regenerating the types.
- JSON costs a serialisation per event; timelines arrive as diffs, so the volume stays small.
  Media never crosses as bytes.
- The Rust build for phones is slow (the SDK and libwebrtc per target) and the debug library is
  large; `ubrn build android --release` for anything shipped.
- iOS is configured but can only be built and checked on a Mac. It needs iOS 17 or later
  (`ios.deploymentTarget`, the push extension and the `call-media` pod), so the iOS 17 capture
  APIs are used without fallbacks.
- Calls on phones, still open: the Kotlin and Swift of `call-media` were written without a
  Gradle or Xcode build here, so the first device build may need fixes. iOS also needs the
  engine's static library to export the `bm_*` symbols to the module's pod (the header is
  `MatrixCallMedia.h`) and `-ObjC` for libwebrtc's categories, and CallKit / Android's
  `ConnectionService` and a foreground service so calls survive the app going to the
  background; incoming calls ring only while the app runs (no VoIP push). Frames are converted
  to RGBA and drawn by the CPU (fine for a handful of tiles; GPU textures would be cheaper).
  Screen sharing from phones isn't offered.
- Push, still open: the extension and its linking were written without Xcode, and the extension
  links the whole engine (calls included), so its memory under iOS's ~24 MB limit must be
  checked on a device (`docs/push.md` lists what to verify). The engine holds the crypto
  store's lock through each long-poll sync, so a push that arrives meanwhile waits for it (the
  SDK's notification client waits up to a minute) and the store's lease is written 20 times a
  second while it syncs; the extension stays out while the app is in front, so the two meet
  only when the app syncs in the background. The extension can't drop a
  push (that needs Apple's notification-filtering entitlement), so a pusher whose removal
  failed may still show a generic notification on iOS until it is removed. iOS may kill a suspended app that holds SQLite locks in the shared container
  (`0xdead10cc`). An extension can't drop a push, so one that turns out to be nothing still
  shows the generic text. Pushes don't update the badge. Calls still don't ring through push.
- Watch: a new boundary type needs the `ffi` derives (the build fails without them); a new event
  needs a case in the reducer (the type check fails without it).
