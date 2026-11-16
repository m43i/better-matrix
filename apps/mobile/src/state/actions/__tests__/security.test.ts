import { detachedEngine } from '@better-matrix/matrix-native/client';

import { ADA, GRACE } from '../../../test/fixtures';
import { CommandFailed } from '../../requests';
import { Store } from '../../store';
import type { Verification } from '../../types';
import {
  endSession,
  followedVerification,
  newIncoming,
  recoveryKeyFor,
  renameSession,
  type ResetStep,
  resetIdentity,
  runGuarded,
  setUpRecovery,
  verificationKey,
  verificationView,
} from '../security';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);
  const finish = (request: number, error?: string) =>
    engine.emit({ type: 'Finished', data: { request, result: error === undefined ? { Ok: null } : { Err: error } } });
  const askPassword = jest.fn<Promise<string | null>, [string | null]>(() => Promise.resolve('secret'));
  return { engine, sent, store, finish, askPassword };
}

describe('runGuarded', () => {
  it('should finish without asking when the server does not guard the action', async () => {
    const { store, sent, finish, askPassword } = setup();
    const done = runGuarded(store, ADA, { type: 'EndSession', deviceId: 'DEV' }, { askPassword });
    finish(sent[0]!.request);
    await expect(done).resolves.toBe(true);
    expect(askPassword).not.toHaveBeenCalled();
  });

  it('should send the action again with the password the server asked for', async () => {
    const { engine, store, sent, finish, askPassword } = setup();
    void runGuarded(store, ADA, { type: 'EndSession', deviceId: 'DEV' }, { askPassword });
    engine.emit({ type: 'PasswordNeeded', data: { request: sent[0]!.request } });
    finish(sent[0]!.request, 'password required');
    await flush();
    expect(sent[1]!.command).toEqual({ type: 'EndSession', data: { account: ADA, device_id: 'DEV', password: 'secret' } });
  });

  it('should ask without an error the first time', async () => {
    const { engine, store, sent, finish, askPassword } = setup();
    void runGuarded(store, ADA, { type: 'ResetIdentity' }, { askPassword });
    engine.emit({ type: 'PasswordNeeded', data: { request: sent[0]!.request } });
    finish(sent[0]!.request, 'password required');
    await flush();
    expect(askPassword).toHaveBeenCalledWith(null);
  });

  it('should ask again with the reason when the password is refused', async () => {
    const { engine, store, sent, finish, askPassword } = setup();
    void runGuarded(store, ADA, { type: 'ResetIdentity' }, { askPassword });
    engine.emit({ type: 'PasswordNeeded', data: { request: sent[0]!.request } });
    finish(sent[0]!.request, 'password required');
    await flush();
    finish(sent[1]!.request, 'Invalid password');
    await flush();
    expect(askPassword).toHaveBeenLastCalledWith('Invalid password');
  });

  it('should give up when the user cancels the password', async () => {
    const { engine, store, sent, finish } = setup();
    const done = runGuarded(store, ADA, { type: 'ResetIdentity' }, { askPassword: () => Promise.resolve(null) });
    engine.emit({ type: 'PasswordNeeded', data: { request: sent[0]!.request } });
    finish(sent[0]!.request, 'password required');
    await expect(done).resolves.toBe(false);
  });

  it('should reject with the server error when it refuses for another reason', async () => {
    const { store, sent, finish, askPassword } = setup();
    const done = runGuarded(store, ADA, { type: 'ResetIdentity' }, { askPassword });
    finish(sent[0]!.request, 'forbidden');
    await expect(done).rejects.toEqual(new CommandFailed('forbidden', sent[0]!.request));
  });

  it('should open the page that approves the action', () => {
    const { engine, store, sent, askPassword } = setup();
    const openUrl = jest.fn();
    void runGuarded(store, ADA, { type: 'ResetIdentity' }, { askPassword, openUrl });
    engine.emit({ type: 'OpenUrl', data: { request: sent[0]!.request, url: 'https://account.example.org/reset' } });
    expect(openUrl).toHaveBeenCalledWith('https://account.example.org/reset');
  });
});

describe('resetIdentity', () => {
  it('should end done once the reset went through', async () => {
    const { store, sent, finish, askPassword } = setup();
    const steps: ResetStep[] = [];
    const done = resetIdentity(store, ADA, { askPassword, openUrl: jest.fn(), onStep: (s) => steps.push(s) });
    finish(sent[0]!.request);
    await expect(done).resolves.toEqual({ type: 'done' });
  });

  it('should wait for the approval on the page it opened', () => {
    const { engine, store, sent, askPassword } = setup();
    const steps: ResetStep[] = [];
    void resetIdentity(store, ADA, { askPassword, openUrl: jest.fn(), onStep: (s) => steps.push(s) });
    engine.emit({ type: 'OpenUrl', data: { request: sent[0]!.request, url: 'https://account.example.org/reset' } });
    expect(steps[steps.length - 1]).toEqual({ type: 'resetting', request: sent[0]!.request, url: 'https://account.example.org/reset' });
  });

  it('should go back to the warning with the error when the reset fails', async () => {
    const { store, sent, finish, askPassword } = setup();
    const done = resetIdentity(store, ADA, { askPassword, openUrl: jest.fn(), onStep: jest.fn() });
    finish(sent[0]!.request, 'no');
    await expect(done).resolves.toEqual({ type: 'warning', error: 'no' });
  });

  it('should go back to the warning without an error when it was cancelled', async () => {
    const { store, sent, finish, askPassword } = setup();
    const done = resetIdentity(store, ADA, { askPassword, openUrl: jest.fn(), onStep: jest.fn() });
    finish(sent[0]!.request, 'cancelled');
    await expect(done).resolves.toEqual({ type: 'warning', error: null });
  });
});

describe('verifications', () => {
  const incoming: Verification = { account: ADA, flowId: 'in', step: { type: 'Incoming', data: { other_device: 'Phone' } } };
  const waiting: Verification = { account: ADA, flowId: 'mine', step: { type: 'Waiting' } };
  const graces: Verification = { account: GRACE, flowId: 'other', step: { type: 'Waiting' } };

  it('should follow the flow it was opened for', () => {
    expect(followedVerification([waiting, incoming], ADA, 'in', [])).toBe(incoming);
  });

  it('should follow the first new flow of the account when opened without one', () => {
    expect(followedVerification([incoming, graces, waiting], ADA, null, ['in'])).toBe(waiting);
  });

  it('should follow nothing until the request it started shows up', () => {
    expect(followedVerification([incoming], ADA, null, ['in'])).toBeUndefined();
  });

  it('should find incoming requests not seen yet', () => {
    expect(newIncoming([incoming, waiting], new Set())).toEqual([incoming]);
  });

  it('should not offer an incoming request seen before', () => {
    expect(newIncoming([incoming], new Set([verificationKey(incoming)]))).toEqual([]);
  });

  it('should offer to accept or decline a request that came in', () => {
    expect(verificationView(incoming.step).actions).toEqual(['decline', 'accept']);
  });

  it('should ask whether the emoji match', () => {
    expect(verificationView({ type: 'Compare', data: [] }).actions).toEqual(['mismatch', 'match']);
  });

  it('should offer to cancel while waiting for the other session', () => {
    expect(verificationView(null)).toEqual({ title: 'verify.title', actions: ['cancel'] });
  });

  it('should say the session is verified when done', () => {
    expect(verificationView({ type: 'Done' })).toEqual({ title: 'verify.done_title', actions: ['done'] });
  });

  it('should only close a stopped verification', () => {
    expect(verificationView({ type: 'Cancelled', data: 'mismatch' }).actions).toEqual(['close']);
  });
});

describe('recovery', () => {
  it('should set up recovery', () => {
    const { store, sent } = setup();
    void setUpRecovery(store, ADA, false);
    expect(sent[0]!.command).toEqual({ type: 'SetUpRecovery', data: ADA });
  });

  it('should replace the key when resetting it', () => {
    const { store, sent } = setup();
    void setUpRecovery(store, ADA, true);
    expect(sent[0]!.command).toEqual({ type: 'ResetRecoveryKey', data: ADA });
  });

  it('should show the new key for its account', async () => {
    const { engine, store } = setup();
    engine.emit({ type: 'RecoveryKey', data: { account: ADA, key: 'EsT0 abcd' } });
    await flush();
    expect(recoveryKeyFor(store.getState(), ADA)).toBe('EsT0 abcd');
  });

  it('should not show a new key for another account', async () => {
    const { engine, store } = setup();
    engine.emit({ type: 'RecoveryKey', data: { account: ADA, key: 'EsT0 abcd' } });
    await flush();
    expect(recoveryKeyFor(store.getState(), GRACE)).toBeNull();
  });
});

describe('sessions', () => {
  it('should list the sessions again after ending one', async () => {
    const { store, sent, finish, askPassword } = setup();
    const ended = endSession(store, ADA, 'DEV', { askPassword });
    finish(sent[0]!.request);
    await ended;
    expect(sent[1]!.command).toEqual({ type: 'ListSessions', data: ADA });
  });

  it('should not rename a session to a blank name', () => {
    const { store, sent } = setup();
    renameSession(store, ADA, 'DEV', '  ');
    expect(sent).toEqual([]);
  });

  it('should rename a session with the trimmed name', () => {
    const { store, sent } = setup();
    renameSession(store, ADA, 'DEV', ' Phone ');
    expect(sent[0]!.command).toEqual({ type: 'RenameSession', data: { account: ADA, device_id: 'DEV', name: 'Phone' } });
  });
});
