import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { CallInfo, Command, IncomingCall } from '@better-matrix/matrix-native/types';

import type { CallPermissions } from '../../engine/callMedia';
import {
  answerCall,
  callEffect,
  declineCall,
  hangUp,
  pinCallTile,
  setCallLayout,
  setCallStrip,
  showCallChat,
  startCall,
  toggleCallSelf,
  toggleCamera,
  toggleMicrophone,
} from '../actions/calls';
import { camera } from '../callLayout';
import { Store } from '../store';

const ADA = '@ada:example.org';

// The phone's permissions, as each test sets them: `null` means the system has to be asked.
let known: CallPermissions | null = { microphone: true, camera: true };

let asked: CallPermissions = { microphone: true, camera: true };

const access = {
  prepare: () => undefined,
  known: () => known,
  ask: () => Promise.resolve(asked),
};

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  store.addEffect(callEffect);
  const commands = () => sent.map((s) => s.command);

  const finish = (request: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  return { engine, store, sent, commands, finish };
}

function call(change: Partial<CallInfo> = {}): CallInfo {
  return {
    call: 1,
    account: ADA,
    room_id: '!r',
    via: 'Rtc',
    phase: 'Connected',
    video: false,
    microphone: true,
    camera: null,
    screen: null,
    participants: [],
    ...change,
  };
}

const ring: IncomingCall = {
  account: ADA,
  room_id: '!r',
  caller: { id: '@g:x', name: 'Grace', avatar: null },
  video: true,
  via: 'Rtc',
};

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  known = { microphone: true, camera: true };
  asked = { microphone: true, camera: true };
});

describe('starting a call', () => {
  it('should start a video call at once when the camera is allowed', () => {
    const { store, commands } = setup();
    void startCall(store, ADA, '!r', true, access);
    expect(commands()).toEqual<Command[]>([
      { type: 'StartCall', data: { account: ADA, room_id: '!r', video: true } },
    ]);
  });

  it('should ask first and start once the microphone is allowed', async () => {
    known = null;
    const { store, commands } = setup();
    const done = startCall(store, ADA, '!r', false, access);
    expect(commands()).toEqual([]);
    await flush();
    expect(commands()).toEqual<Command[]>([
      { type: 'StartCall', data: { account: ADA, room_id: '!r', video: false } },
    ]);
    void done;
  });

  it('should not start without the microphone and say why', async () => {
    known = null;
    asked = { microphone: false, camera: false };
    const { store, commands } = setup();
    await startCall(store, ADA, '!r', false, access);
    expect(commands()).toEqual([]);
    expect(store.getState().notices.map((n) => n.message)).toEqual([
      'Calls need the microphone. Allow it in the system settings.',
    ]);
  });

  it('should start without video when the camera is refused', async () => {
    known = null;
    asked = { microphone: true, camera: false };
    const { store, commands } = setup();
    void startCall(store, ADA, '!r', true, access);
    await flush();
    expect(commands()).toEqual<Command[]>([
      { type: 'StartCall', data: { account: ADA, room_id: '!r', video: false } },
    ]);
  });

  it('should word a failed start as the desktop does', async () => {
    const { store, sent, finish } = setup();
    const done = startCall(store, ADA, '!r', false, access);
    finish(sent[0]!.request, 'no call service');
    await done;
    expect(store.getState().notices.map((n) => n.message)).toEqual([
      'The call failed: no call service',
    ]);
  });
});

describe('rings', () => {
  it('should answer a ring with video and take its card away', () => {
    const { engine, store, commands } = setup();
    engine.emit({ type: 'IncomingCall', data: ring });
    void answerCall(store, ADA, '!r', true, access);
    expect(store.getState().call.rings).toEqual([]);
    expect(commands()).toEqual<Command[]>([
      { type: 'AnswerCall', data: { account: ADA, room_id: '!r', video: true } },
    ]);
  });

  it('should decline a ring it cannot answer without the microphone', async () => {
    known = null;
    asked = { microphone: false, camera: false };
    const { store, commands } = setup();
    void answerCall(store, ADA, '!r', false, access);
    await flush();
    expect(commands()).toEqual<Command[]>([
      { type: 'DeclineCall', data: { account: ADA, room_id: '!r' } },
    ]);
  });

  it('should decline a ring and take its card away', () => {
    const { engine, store, commands } = setup();
    engine.emit({ type: 'IncomingCall', data: ring });
    void declineCall(store, ADA, '!r');
    expect(store.getState().call.rings).toEqual([]);
    expect(commands()).toEqual<Command[]>([
      { type: 'DeclineCall', data: { account: ADA, room_id: '!r' } },
    ]);
  });
});

describe('the call under way', () => {
  it('should mute an open microphone', () => {
    const { engine, store, commands } = setup();
    engine.emit({ type: 'CallUpdated', data: call() });
    void toggleMicrophone(store);
    expect(commands()).toEqual<Command[]>([
      { type: 'SetCallMedia', data: { type: 'Microphone', data: false } },
    ]);
  });

  it('should turn the camera on when it is off', () => {
    const { engine, store, commands } = setup();
    engine.emit({ type: 'CallUpdated', data: call() });
    void toggleCamera(store, access);
    expect(commands()).toEqual<Command[]>([
      { type: 'SetCallMedia', data: { type: 'Camera', data: true } },
    ]);
  });

  it('should turn a filming camera off without asking for anything', () => {
    known = null;
    const { engine, store, commands } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ camera: 3 }) });
    void toggleCamera(store, access);
    expect(commands()).toEqual<Command[]>([
      { type: 'SetCallMedia', data: { type: 'Camera', data: false } },
    ]);
  });

  it('should hang up', () => {
    const { store, commands } = setup();
    void hangUp(store);
    expect(commands()).toEqual<Command[]>([{ type: 'HangUp' }]);
  });
});

describe('notices', () => {
  it('should say a call was declined when it ends that way', () => {
    const { engine, store } = setup();
    engine.emit({ type: 'CallUpdated', data: call() });
    engine.emit({
      type: 'CallEnded',
      data: { account: ADA, room_id: '!r', call: 1, reason: { type: 'Declined' } },
    });
    expect(store.getState().notices.map((n) => n.message)).toEqual(['The call was declined.']);
  });

  it('should say nothing when a replaced call ends', () => {
    const { engine, store } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ call: 2 }) });
    engine.emit({
      type: 'CallEnded',
      data: { account: ADA, room_id: '!r', call: 1, reason: { type: 'Declined' } },
    });
    expect(store.getState().notices).toEqual([]);
  });

  it('should show why the camera failed to turn on', () => {
    const { engine, store } = setup();
    engine.emit({ type: 'CallMediaFailed', data: 'camera busy' });
    expect(store.getState().notices.map((n) => n.message)).toEqual([
      "Couldn't turn that on: camera busy",
    ]);
  });
});

describe('the call view', () => {
  const grace = {
    key: 'grace',
    person: { id: '@g:x', name: 'Grace', avatar: null },
    muted: false,
    camera: 1,
    screen: null,
  };

  it('should start a call with the automatic layout and self-view', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    expect(store.getState().call.view).toMatchObject({
      layout: { type: 'Auto' },
      showSelf: true,
      chat: false,
      page: 0,
    });
  });

  it('choosing a layout should start at its first page', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    store.update((s) => ({ ...s, call: { ...s.call, view: { ...s.call.view, page: 3 } } }));
    setCallLayout(store, { type: 'Grid' });
    expect([store.getState().call.view.layout, store.getState().call.view.page]).toEqual([
      { type: 'Grid' },
      0,
    ]);
  });

  it('selecting a tile should pin that surface', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    pinCallTile(store, camera('grace'));
    expect(store.getState().call.view.layout).toEqual({ type: 'Focus', pin: camera('grace') });
  });

  it('a pinned participant leaving should restore the automatic layout', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    pinCallTile(store, camera('grace'));
    engine.emit({ type: 'CallUpdated', data: call() });
    expect(store.getState().call.view.layout).toEqual({ type: 'Auto' });
  });

  it('the same call should keep the view through updates', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    toggleCallSelf(store);
    showCallChat(store, true);
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace], microphone: false }) });
    expect(store.getState().call.view).toMatchObject({ showSelf: false, chat: true });
  });

  it('another call should start from the default view', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    showCallChat(store, true);
    engine.emit({ type: 'CallUpdated', data: call({ call: 2, participants: [grace] }) });
    expect(store.getState().call.view.chat).toBe(false);
  });

  it('the end of a call should forget its view', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    setCallLayout(store, { type: 'Grid' });
    engine.emit({
      type: 'CallEnded',
      data: { account: ADA, room_id: '!r', call: 1, reason: { type: 'HungUp' } },
    });
    expect(store.getState().call.view.layout).toEqual({ type: 'Auto' });
  });

  it('should change no view without a call', () => {
    const { store } = setup();
    const before = store.getState();
    showCallChat(store, true);
    expect(store.getState()).toBe(before);
  });

  it('dragging the divider should keep the strip within its bounds', () => {
    const { store, engine } = setup();
    engine.emit({ type: 'CallUpdated', data: call({ participants: [grace] }) });
    setCallStrip(store, 0.9);
    expect(store.getState().call.view.strip).toBe(0.4);
  });
});
