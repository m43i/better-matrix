import { detachedEngine } from '@better-matrix/matrix-native/client';
import type { Homeserver, QrLoginStep } from '@better-matrix/matrix-native/types';

import { ADA, GRACE, summary } from '../../../test/fixtures';
import { Store } from '../../store';
import {
  formatCheckCode,
  linkDevice,
  parseCheckCode,
  QrLoginFlow,
  qrLoginView,
  signInWithQr,
} from '../qrLogin';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const server = (qr: boolean): Homeserver => ({
  server_name: 'example.org',
  url: 'https://matrix.example.org/',
  methods: { oauth: true, password: false, create_account: false, qr },
});

function setup(start = linkDevice(ADA)) {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const opened: string[] = [];
  const flow = new QrLoginFlow(store, start, (url) => opened.push(url));

  const step = (request: number, qrStep: QrLoginStep) =>
    engine.emit({ type: 'QrLogin', data: { request, step: qrStep } });

  const finish = (request: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  return { engine, sent, store, flow, opened, step, finish };
}

describe('qrLoginView', () => {
  it('should show the code to scan', () => {
    const qr = { width: 1, dark: [true] };
    expect(qrLoginView({ type: 'Show', data: qr })).toEqual({ type: 'show', qr });
  });

  it('should ask for the check code with nothing sent yet', () => {
    expect(qrLoginView({ type: 'EnterCheckCode' })).toEqual({
      type: 'enterCheckCode',
      sent: false,
    });
  });
});

describe('check codes', () => {
  it('should read two digits', () => expect(parseCheckCode(' 07 ')).toBe(7));
  it('should refuse anything but two digits', () => expect(parseCheckCode('7')).toBeNull());
  it('should show a check code as two digits', () => expect(formatCheckCode(7)).toBe('07'));
});

describe('the commands that start it', () => {
  it('should link a device by showing a code', () => {
    expect(linkDevice(ADA).shown).toEqual({
      type: 'LinkDevice',
      data: { account: ADA, scanned: null },
    });
  });

  it('should link a device with the code it scanned', () => {
    expect(linkDevice(ADA).scanned([1, 2])).toEqual({
      type: 'LinkDevice',
      data: { account: ADA, scanned: [1, 2] },
    });
  });

  it('should sign in with a scanned code without a server', () => {
    expect(signInWithQr(null).scanned([4])).toEqual({
      type: 'SignInWithQr',
      data: { type: 'Scanned', data: [4] },
    });
  });

  it('should show a code through a server that takes QR sign-ins', () => {
    expect(signInWithQr(server(true)).shown).toEqual({
      type: 'SignInWithQr',
      data: { type: 'Show', data: { homeserver: 'https://matrix.example.org/' } },
    });
  });

  it('should not show a code without a server that takes QR sign-ins', () => {
    expect(signInWithQr(server(false)).shown).toBeNull();
  });
});

describe('QrLoginFlow', () => {
  it('should send nothing while the camera looks for a code', () => {
    const { flow, sent } = setup();
    flow.scan();
    expect([flow.snapshot(), sent.length]).toEqual([{ type: 'scanning' }, 0]);
  });

  it('should start with the code the camera read', () => {
    const { flow, sent } = setup();
    flow.scan();
    flow.scanned([5, 6]);
    expect(sent[0]!.command).toEqual({
      type: 'LinkDevice',
      data: { account: ADA, scanned: [5, 6] },
    });
  });

  it('should follow the steps the engine reports', () => {
    const { flow, sent, step } = setup();
    flow.show();
    step(sent[0]!.request, { type: 'CheckCode', data: 42 });
    expect(flow.snapshot()).toEqual({ type: 'checkCode', code: 42 });
  });

  it('should answer with the check code typed', () => {
    const { flow, sent, step } = setup();
    flow.show();
    const request = sent[0]!.request;
    step(request, { type: 'EnterCheckCode' });
    expect(flow.enterCheckCode('09')).toBe(true);
    expect(sent[1]!.command).toEqual({
      type: 'Answer',
      data: { request, answer: { type: 'CheckCode', data: 9 } },
    });
  });

  it('should not answer with a check code that is not two digits', () => {
    const { flow, sent, step } = setup();
    flow.show();
    step(sent[0]!.request, { type: 'EnterCheckCode' });
    expect([flow.enterCheckCode('9'), sent.length]).toEqual([false, 1]);
  });

  it('should open the account page the engine asks for', () => {
    const { engine, flow, sent, opened } = setup();
    flow.show();
    engine.emit({ type: 'OpenUrl', data: { request: sent[0]!.request, url: 'https://approve' } });
    expect(opened).toEqual(['https://approve']);
  });

  it('should cancel the exchange under way', () => {
    const { flow, sent } = setup();
    flow.show();
    flow.cancel();
    expect(sent[1]!.command).toEqual({ type: 'Cancel', data: sent[0]!.request });
  });

  it('should say why it stopped', async () => {
    const { flow, sent, finish } = setup();
    flow.show();
    finish(sent[0]!.request, 'the code is from the wrong side');
    await flush();
    expect(flow.snapshot()).toEqual({ type: 'failed', error: 'the code is from the wrong side' });
  });

  it('should not be done when the engine reports done before the command finished', () => {
    const { flow, sent, step } = setup(signInWithQr(null));
    flow.scan();
    flow.scanned([1]);
    step(sent[0]!.request, { type: 'Done', data: { account: null } });
    expect(flow.snapshot()).toEqual({ type: 'finishing', account: null });
  });

  it('should not cancel a sign-in the engine reported done', () => {
    const { flow, sent, step } = setup(signInWithQr(null));
    flow.scan();
    flow.scanned([1]);
    step(sent[0]!.request, { type: 'Done', data: { account: null } });
    flow.cancel();
    expect(sent.map((s) => s.command.type)).toEqual(['SignInWithQr']);
  });

  it('should be done once the command finished after the engine reported done', async () => {
    const { flow, sent, step, finish } = setup(signInWithQr(null));
    flow.scan();
    flow.scanned([1]);
    step(sent[0]!.request, { type: 'Done', data: { account: null } });
    finish(sent[0]!.request);
    await flush();
    expect(flow.snapshot()).toEqual({ type: 'done', account: null });
  });

  it('should put the account it signed in on screen', async () => {
    const { engine, flow, sent, step, finish, store } = setup(signInWithQr(null));
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    flow.scan();
    flow.scanned([1]);
    engine.emit({ type: 'AccountAdded', data: summary(GRACE) });
    step(sent[0]!.request, { type: 'Done', data: { account: GRACE } });
    finish(sent[0]!.request);
    await flush();
    expect([flow.snapshot(), store.getState().active]).toEqual([
      { type: 'done', account: GRACE },
      GRACE,
    ]);
  });

  it('should not put an account restored meanwhile on screen', async () => {
    const ALAN = '@alan:example.org';
    const { engine, flow, sent, step, finish, store } = setup(signInWithQr(null));
    engine.emit({ type: 'AccountAdded', data: summary(ADA) });
    flow.scan();
    flow.scanned([1]);
    engine.emit({ type: 'AccountAdded', data: summary(GRACE) });
    engine.emit({ type: 'AccountAdded', data: summary(ALAN) });
    step(sent[0]!.request, { type: 'Done', data: { account: ALAN } });
    finish(sent[0]!.request);
    await flush();
    expect(store.getState().active).toEqual(ALAN);
  });

  it('should send a check code only once', () => {
    const { flow, sent, step } = setup();
    flow.show();
    step(sent[0]!.request, { type: 'EnterCheckCode' });
    flow.enterCheckCode('42');
    expect([
      flow.enterCheckCode('42'),
      sent.filter((s) => s.command.type === 'Answer').length,
    ]).toEqual([false, 1]);
  });
});
