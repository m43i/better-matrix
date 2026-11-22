import { detachedEngine } from '@better-matrix/matrix-native/client';

import { ADA, GRACE } from '../../../test/fixtures';
import { CommandFailed } from '../../requests';
import { Store } from '../../store';
import type { Verification } from '../../types';
import {
  AskedVerification,
  changePassword,
  compareEmoji,
  confirmationProblem,
  deactivateAccount,
  endSession,
  exportRoomKeys,
  followedVerification,
  importRoomKeysAsking,
  loadIdentity,
  newIncoming,
  readQrCode,
  recoveryKeyFor,
  renameSession,
  type ResetStep,
  resetIdentity,
  runGuarded,
  scanVerificationCode,
  setOfflineDevice,
  setUpRecovery,
  turnOnOfflineDevice,
  untilAccepted,
  verificationKey,
  verificationView,
} from '../security';

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup() {
  const { engine, sent } = detachedEngine();
  const store = new Store(engine);

  const finish = (request: number, error?: string) =>
    engine.emit({
      type: 'Finished',
      data: { request, result: error === undefined ? { Ok: null } : { Err: error } },
    });

  const askPassword = jest.fn<Promise<string | null>, [string | null]>(() =>
    Promise.resolve('secret'),
  );

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
    expect(sent[1]!.command).toEqual({
      type: 'EndSession',
      data: { account: ADA, device_id: 'DEV', password: 'secret' },
    });
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

    const done = runGuarded(
      store,
      ADA,
      { type: 'ResetIdentity' },
      { askPassword: () => Promise.resolve(null) },
    );

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
    engine.emit({
      type: 'OpenUrl',
      data: { request: sent[0]!.request, url: 'https://account.example.org/reset' },
    });
    expect(openUrl).toHaveBeenCalledWith('https://account.example.org/reset');
  });
});

describe('resetIdentity', () => {
  it('should end done once the reset went through', async () => {
    const { store, sent, finish, askPassword } = setup();
    const steps: ResetStep[] = [];

    const done = resetIdentity(store, ADA, {
      askPassword,
      openUrl: jest.fn(),
      onStep: (s) => steps.push(s),
    });

    finish(sent[0]!.request);
    await expect(done).resolves.toEqual({ type: 'done' });
  });

  it('should wait for the approval on the page it opened', () => {
    const { engine, store, sent, askPassword } = setup();
    const steps: ResetStep[] = [];
    void resetIdentity(store, ADA, {
      askPassword,
      openUrl: jest.fn(),
      onStep: (s) => steps.push(s),
    });
    engine.emit({
      type: 'OpenUrl',
      data: { request: sent[0]!.request, url: 'https://account.example.org/reset' },
    });
    expect(steps[steps.length - 1]).toEqual({
      type: 'resetting',
      request: sent[0]!.request,
      url: 'https://account.example.org/reset',
    });
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
  const incoming: Verification = {
    account: ADA,
    flowId: 'in',
    step: { type: 'Incoming', data: { other_device: 'Phone', user: null } },
    user: null,
  };

  const waiting: Verification = {
    account: ADA,
    flowId: 'mine',
    step: { type: 'Waiting' },
    user: null,
  };

  const graces: Verification = {
    account: GRACE,
    flowId: 'other',
    step: { type: 'Waiting' },
    user: null,
  };

  it('should follow the flow it was opened for', () => {
    expect(followedVerification([waiting, incoming], ADA, 'in')).toBe(incoming);
  });

  it('should follow nothing until the flow it asked for is known', () => {
    expect(followedVerification([incoming, waiting], ADA, null)).toBeUndefined();
  });

  it('should not follow a flow of the same ID on another account', () => {
    expect(followedVerification([graces], ADA, 'other')).toBeUndefined();
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
    expect(verificationView({ type: 'Done' })).toEqual({
      title: 'verify.done_title',
      actions: ['done'],
    });
  });

  it('should only close a stopped verification', () => {
    expect(verificationView({ type: 'Cancelled', data: 'mismatch' }).actions).toEqual(['close']);
  });

  it('should offer to scan their code or compare emoji once both sides are ready', () => {
    expect(verificationView({ type: 'Ready', data: { qr: null, scan: true } }).actions).toEqual([
      'scan',
      'emoji',
      'cancel',
    ]);
  });

  it('should not offer to scan a code the other side does not show', () => {
    expect(
      verificationView({ type: 'Ready', data: { qr: { width: 1, dark: [true] }, scan: false } })
        .actions,
    ).toEqual(['emoji', 'cancel']);
  });

  it('should ask whether the other side scanned the code', () => {
    expect(verificationView({ type: 'Scanned' }).actions).toEqual(['notScanned', 'scanned']);
  });

  it('should title a verification with another person by their name', () => {
    expect(verificationView({ type: 'Waiting' }, true).title).toBe(
      'mobile.accounts.verify_person_title',
    );
  });

  it('should say the person is verified when done', () => {
    expect(verificationView({ type: 'Done' }, true).title).toBe(
      'mobile.accounts.verify_person_done_title',
    );
  });
});

describe('verifying people and codes', () => {
  it('should ask another person to verify', () => {
    const { store, sent } = setup();
    new AskedVerification(store, ADA, GRACE).start();
    expect(sent[0]!.command).toEqual({
      type: 'VerifyUser',
      data: { account: ADA, user_id: GRACE },
    });
  });

  it('should ask what the user knows of someone’s identity', () => {
    const { store, sent } = setup();
    loadIdentity(store, ADA, GRACE);
    expect(sent[0]!.command).toEqual({
      type: 'LoadIdentity',
      data: { account: ADA, user_id: GRACE },
    });
  });

  it('should switch a verification to emoji', () => {
    const { store, sent } = setup();
    compareEmoji(store, ADA, 'f');
    expect(sent[0]!.command).toEqual({
      type: 'CompareEmoji',
      data: { account: ADA, flow_id: 'f' },
    });
  });

  it('should hand a verification the code the camera read', () => {
    const { store, sent } = setup();
    scanVerificationCode(store, ADA, 'f', [1, 2, 3]);
    expect(sent[0]!.command).toEqual({
      type: 'ScanVerificationCode',
      data: { account: ADA, flow_id: 'f', code: [1, 2, 3] },
    });
  });

  it('should read a picture’s code in the engine', async () => {
    const { engine, store, sent, finish } = setup();
    const read = readQrCode(store, '/cache/picture.jpg');
    const { request, command } = sent[0]!;
    expect(command).toEqual({ type: 'ReadQrCode', data: { picture: '/cache/picture.jpg' } });
    engine.emit({ type: 'QrCodeRead', data: { request, code: [9, 8] } });
    finish(request);
    await expect(read).resolves.toEqual([9, 8]);
  });

  it('should read nothing from a picture without a code', async () => {
    const { engine, store, sent, finish } = setup();
    const read = readQrCode(store, '/cache/picture.jpg');
    engine.emit({ type: 'QrCodeRead', data: { request: sent[0]!.request, code: null } });
    finish(sent[0]!.request);
    await expect(read).resolves.toBeNull();
  });
});

describe('the offline device', () => {
  it('should turn the offline device on with the recovery key', () => {
    const { store, sent } = setup();
    void setOfflineDevice(store, ADA, true, ' EsT0 abcd ');
    expect(sent[0]!.command).toEqual({
      type: 'SetOfflineDevice',
      data: { account: ADA, on: true, recovery_key: 'EsT0 abcd' },
    });
  });

  it('should turn the offline device off without a key', () => {
    const { store, sent } = setup();
    void setOfflineDevice(store, ADA, false, 'EsT0');
    expect(sent[0]!.command).toEqual({
      type: 'SetOfflineDevice',
      data: { account: ADA, on: false, recovery_key: null },
    });
  });

  it('should ask for the recovery key again when it is wrong', async () => {
    const { store, sent, finish } = setup();
    const ask = jest.fn<Promise<string | null>, [string | null]>(() => Promise.resolve('key'));
    void turnOnOfflineDevice(store, ADA, ask);
    await flush();
    finish(sent[0]!.request, 'that recovery key is not right');
    await flush();
    expect(ask).toHaveBeenLastCalledWith('that recovery key is not right');
  });

  it('should stop asking once the user gives up', async () => {
    const { store } = setup();
    await expect(turnOnOfflineDevice(store, ADA, () => Promise.resolve(null))).resolves.toBe(false);
  });
});

describe('untilAccepted', () => {
  it('should resolve with what the accepted attempt resolved with', async () => {
    await expect(
      untilAccepted(
        () => Promise.resolve('secret'),
        (secret) => Promise.resolve(secret.length),
      ),
    ).resolves.toBe(6);
  });

  it('should end when an attempt is cancelled', async () => {
    await expect(
      untilAccepted(
        () => Promise.resolve('secret'),
        () => Promise.reject(new CommandFailed('cancelled', 1)),
      ),
    ).rejects.toThrow('cancelled');
  });
});

describe('message keys', () => {
  it('should export the keys to the file with the passphrase', () => {
    const { store, sent } = setup();
    void exportRoomKeys(store, ADA, '/cache/keys.txt', 'horse battery');
    expect(sent[0]!.command).toEqual({
      type: 'ExportRoomKeys',
      data: { account: ADA, path: '/cache/keys.txt', passphrase: 'horse battery' },
    });
  });

  it('should import the keys and say how many were new', async () => {
    const { engine, store, sent, finish } = setup();
    const done = importRoomKeysAsking(store, ADA, '/cache/keys.txt', () => Promise.resolve('pass'));
    await flush();
    const { request, command } = sent[0]!;
    expect(command).toEqual({
      type: 'ImportRoomKeys',
      data: { account: ADA, path: '/cache/keys.txt', passphrase: 'pass' },
    });
    engine.emit({ type: 'KeysImported', data: { request, import: { imported: 3, total: 5 } } });
    finish(request);
    await expect(done).resolves.toEqual({ imported: 3, total: 5 });
  });

  it('should want a passphrase', () => {
    expect(confirmationProblem('', '')).toBe('empty');
  });

  it('should want the passphrase typed the same twice', () => {
    expect(confirmationProblem('horse', 'house')).toBe('mismatch');
  });

  it('should take a passphrase typed the same twice', () => {
    expect(confirmationProblem('horse', 'horse')).toBeNull();
  });
});

describe('password and deactivation', () => {
  it('should change the password with the current one', () => {
    const { store, sent } = setup();
    void changePassword(store, ADA, 'old', 'new');
    expect(sent[0]!.command).toEqual({
      type: 'ChangePassword',
      data: { account: ADA, new_password: 'new', password: 'old' },
    });
  });

  it('should reject with why the server refused the current password', async () => {
    const { store, sent, finish } = setup();
    const done = changePassword(store, ADA, 'wrong', 'new');
    finish(sent[0]!.request, 'Invalid password');
    await expect(done).rejects.toThrow('Invalid password');
  });

  it('should deactivate with the password the server asked for', async () => {
    const { engine, store, sent, finish, askPassword } = setup();
    void deactivateAccount(store, ADA, true, { askPassword });
    engine.emit({ type: 'PasswordNeeded', data: { request: sent[0]!.request } });
    finish(sent[0]!.request, 'password required');
    await flush();
    expect(sent[1]!.command).toEqual({
      type: 'DeactivateAccount',
      data: { account: ADA, erase: true, password: 'secret' },
    });
  });

  it('should open the account page an OAuth account deactivates on', async () => {
    const { engine, store, sent, finish, askPassword } = setup();
    const openUrl = jest.fn();
    const done = deactivateAccount(store, ADA, false, { askPassword, openUrl });
    engine.emit({
      type: 'OpenUrl',
      data: { request: sent[0]!.request, url: 'https://account.example.org/deactivate' },
    });
    finish(sent[0]!.request);
    await expect(done).resolves.toBe(true);
    expect(openUrl).toHaveBeenCalledWith('https://account.example.org/deactivate');
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
    expect(sent[0]!.command).toEqual({
      type: 'RenameSession',
      data: { account: ADA, device_id: 'DEV', name: 'Phone' },
    });
  });
});

describe('AskedVerification', () => {
  /** Ada's request to verify Alice's identity, with the store's verifications as followed. */
  function asking(userId: string | null = '@alice:example.org') {
    const env = setup();
    const asked = new AskedVerification(env.store, ADA, userId);
    asked.start();
    const request = env.sent[0]!.request;

    const step = (flowId: string, verificationStep: Verification['step']) =>
      env.engine.emit({
        type: 'Verification',
        data: { account: ADA, flow_id: flowId, step: verificationStep },
      });

    const requested = (flowId: string) =>
      env.engine.emit({
        type: 'VerificationRequested',
        data: { request, account: ADA, flow_id: flowId },
      });

    const followed = () =>
      followedVerification(env.store.getState().verifications, ADA, asked.flowId());

    return { ...env, asked, request, step, requested, followed };
  }

  it('should verify this session without a person', () => {
    const { sent } = asking(null);
    expect(sent[0]!.command).toEqual({ type: 'VerifyThisSession', data: ADA });
  });

  it('should bind to the flow its request started', () => {
    const { asked, requested } = asking();
    requested('alice-flow');
    expect(asked.flowId()).toBe('alice-flow');
  });

  it('should not be taken over by a request from Bob arriving while waiting on Alice', () => {
    const { step, requested, followed } = asking();
    step('bob-flow', {
      type: 'Incoming',
      data: {
        other_device: 'BOBPHONE',
        user: { id: '@bob:example.org', name: 'Bob', avatar: null },
      },
    });
    requested('alice-flow');
    step('alice-flow', { type: 'Waiting' });
    expect(followed()?.flowId).toBe('alice-flow');
  });

  it('should follow nothing before its request named a flow, whatever else comes in', () => {
    const { step, followed } = asking();
    step('bob-flow', { type: 'Waiting' });
    expect(followed()).toBeUndefined();
  });

  it('should cancel only its own flow when closed', () => {
    const { asked, sent, step, requested } = asking();
    step('bob-flow', { type: 'Waiting' });
    requested('alice-flow');
    step('alice-flow', { type: 'Waiting' });
    asked.close();
    expect(sent.slice(1).map((s) => s.command)).toEqual([
      { type: 'CancelVerification', data: { account: ADA, flow_id: 'alice-flow' } },
    ]);
  });

  it('should not cancel a request still on its way when closed', () => {
    const { asked, sent } = asking();
    asked.close();
    expect(sent).toHaveLength(1);
  });

  it('should cancel the flow a request closed before named it started', () => {
    const { asked, sent, requested } = asking();
    asked.close();
    requested('alice-flow');
    expect(sent[1]!.command).toEqual({
      type: 'CancelVerification',
      data: { account: ADA, flow_id: 'alice-flow' },
    });
  });

  it('should leave a finished verification alone when closed', () => {
    const { asked, sent, step, requested } = asking();
    requested('alice-flow');
    step('alice-flow', { type: 'Done' });
    asked.close();
    expect(sent).toHaveLength(1);
  });

  it('should say why the request failed', async () => {
    const { store, request, finish } = asking();
    finish(request, 'no identity');
    await flush();
    expect(store.getState().notices.map((n) => n.message)).toContain('no identity');
  });
});
