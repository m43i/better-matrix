// Calls: starting, joining, answering and declining them, the user's media in the call under
// way, and the notices calls give (the desktop's `State::start_call` and friends). The phone's
// permissions and native media are asked for first (`engine/callMedia`).

import type { EngineEvent } from '@better-matrix/matrix-native/client';
import type { AccountId, CallInfo, Command } from '@better-matrix/matrix-native/types';

import { askCallPermissions, type CallPermissions, cameraNames, knownPermissions, prepareCallMedia } from '../../engine/callMedia';
import { t } from '../../i18n';
import { endedNotice, nextCamera, stoppedNotice } from '../../ui/call/stage';
import type { CallLayout, CallPlan, CallTile } from '../callLayout';
import { type CallView, paged, pinned, toggledFit, toggledSelf, withLayout, withStrip } from '../callView';
import { addNotice } from '../reducer';
import type { CommandFailed } from '../requests';
import type { Store } from '../store';
import type { State } from '../types';

/** Sends a call command; a failure shows as the desktop's "The call failed: …". */
function dispatch(store: Store, command: Command): Promise<void> {
  return store.run(command).catch((error: CommandFailed) => {
    if (!error.cancelled) notice(store, t('call.failed', { error: error.message }));
  });
}

function notice(store: Store, message: string): void {
  store.update((s) => addNotice(s, message));
}

/** Whether the call can go ahead, and with video, given what the user allowed. */
function decide(store: Store, video: boolean, allowed: CallPermissions): { video: boolean } | null {
  if (!allowed.microphone) {
    notice(store, t('mobile.calls.microphone_needed'));
    return null;
  }
  if (video && !allowed.camera) notice(store, t('mobile.calls.camera_needed'));
  return { video: video && allowed.camera };
}

/**
 * Gets the native media ready and the microphone (and camera for `video`) allowed, then runs
 * `go`; at once when nothing has to be asked, so the command goes out without a wait.
 */
function whenReady(store: Store, video: boolean, go: (video: boolean) => Promise<void>, refused?: () => Promise<void>): Promise<void> {
  prepareCallMedia();
  const proceed = (allowed: CallPermissions) => {
    const decided = decide(store, video, allowed);
    return decided ? go(decided.video) : (refused?.() ?? Promise.resolve());
  };
  const known = knownPermissions(video);
  return known ? proceed(known) : askCallPermissions(video).then(proceed);
}

/** Starts a call in a room, or joins the one under way there (the engine tells them apart). */
export function startCall(store: Store, account: AccountId, roomId: string, video: boolean): Promise<void> {
  return whenReady(store, video, (withVideo) =>
    dispatch(store, { type: 'StartCall', data: { account, room_id: roomId, video: withVideo } }),
  );
}

/** Joins the call under way in a room (a call card's Join). */
export const joinCall = startCall;

function withoutRing(account: AccountId, roomId: string) {
  return (s: State): State => ({
    ...s,
    call: { ...s.call, rings: s.call.rings.filter((r) => !(r.account === account && r.room_id === roomId)) },
  });
}

/** Answers the call ringing in a room; its card goes at once. Without the microphone, declines. */
export function answerCall(store: Store, account: AccountId, roomId: string, video: boolean): Promise<void> {
  store.update(withoutRing(account, roomId));
  return whenReady(
    store,
    video,
    (withVideo) => dispatch(store, { type: 'AnswerCall', data: { account, room_id: roomId, video: withVideo } }),
    () => dispatch(store, { type: 'DeclineCall', data: { account, room_id: roomId } }),
  );
}

/** Declines the call ringing in a room. */
export function declineCall(store: Store, account: AccountId, roomId: string): Promise<void> {
  store.update(withoutRing(account, roomId));
  return dispatch(store, { type: 'DeclineCall', data: { account, room_id: roomId } });
}

export function hangUp(store: Store): Promise<void> {
  return dispatch(store, { type: 'HangUp' });
}

/** Mutes or unmutes the user in the call under way. */
export function toggleMicrophone(store: Store): Promise<void> {
  const current = store.getState().call.current;
  if (!current) return Promise.resolve();
  return dispatch(store, { type: 'SetCallMedia', data: { type: 'Microphone', data: !current.microphone } });
}

/** Turns the user's camera on or off in the call under way. */
export function toggleCamera(store: Store): Promise<void> {
  const current = store.getState().call.current;
  if (!current) return Promise.resolve();
  const on = current.camera === null;
  const send = () => dispatch(store, { type: 'SetCallMedia', data: { type: 'Camera', data: on } });
  if (!on) return send();
  prepareCallMedia();
  const go = (allowed: CallPermissions) => {
    if (allowed.camera) return send();
    notice(store, t('mobile.calls.camera_needed'));
    return Promise.resolve();
  };
  const known = knownPermissions(true);
  return known ? go(known) : askCallPermissions(true).then(go);
}

/** The camera the user switched to, by name; `null` while it's the phone's default. */
let camera: string | null = null;

/** Switches to the phone's next camera (front, back, …), for this call and the next. */
export function switchCamera(store: Store): Promise<void> {
  const next = nextCamera(cameraNames(), camera);
  if (next === null) return Promise.resolve();
  camera = next;
  return dispatch(store, { type: 'ChooseCallDevice', data: { kind: 'Camera', id: next } });
}

/** The notice an engine event about calls gives, if any. */
export function callNotice(event: EngineEvent): string | null {
  switch (event.type) {
    case 'CallEnded':
      return endedNotice(event.data.reason);
    case 'CallMediaFailed':
      return t('call.media_failed', { error: event.data });
    case 'CallMediaStopped':
      return stoppedNotice(event.data);
    default:
      return null;
  }
}

/** The store effect that shows calls' notices. A replaced call's end says nothing. */
export function callEffect(event: EngineEvent, store: Store): void {
  if (event.type === 'CallEnded' && store.getState().call.current !== null) return;
  const message = callNotice(event);
  if (message !== null) notice(store, message);
}

// ----- how the user sees the call (the desktop's `set_call_layout`, `page_call`, …) -----------

/** Changes the view of the call under way; nothing without a call. */
function updateView(store: Store, change: (view: CallView, info: CallInfo) => CallView): void {
  store.update((s) => (s.call.current ? { ...s, call: { ...s.call, view: change(s.call.view, s.call.current) } } : s));
}

/** Auto, Grid or Focus (following the speaker), from the call layout menu. */
export function setCallLayout(store: Store, layout: CallLayout): void {
  updateView(store, (view) => withLayout(view, layout));
}

/** Selecting a tile pins that surface in Focus. */
export function pinCallTile(store: Store, tile: CallTile): void {
  updateView(store, (view) => pinned(view, tile));
}

/** Hides or shows self-view; the camera keeps filming either way. */
export function toggleCallSelf(store: Store): void {
  updateView(store, toggledSelf);
}

/** Switches a camera between Full video and Fill tile. */
export function toggleCallFit(store: Store, tile: CallTile): void {
  updateView(store, (view, info) => toggledFit(info, view, tile));
}

/** The next (`forward`) or previous page of the plan on screen. */
export function pageCall(store: Store, planned: CallPlan, forward: boolean): void {
  updateView(store, (view) => paged(view, planned, forward));
}

/** The participant strip's share of the stage, after dragging its divider. */
export function setCallStrip(store: Store, strip: number): void {
  updateView(store, (view) => withStrip(view, strip));
}

/** Shows the call's room with a compact stage over its chat (`true`), or only the stage. */
export function showCallChat(store: Store, chat: boolean): void {
  updateView(store, (view) => (view.chat === chat ? view : { ...view, chat }));
}
