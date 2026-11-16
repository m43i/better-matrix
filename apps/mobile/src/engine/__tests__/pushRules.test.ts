import { apply, initialState } from '../../state/reducer';
import type { PusherRegistration } from '../../state/types';
import { ADA, GRACE, roomSummary, summary } from '../../test/fixtures';
import {
  badgeCount,
  isPush,
  leftToPush,
  notifyUrl,
  plan,
  PUSH_APP_ID,
  pusherOf,
  Seen,
  targetOfPush,
  unavailable,
  carryOut,
  type PlanSteps,
  pushAllowed,
  tokenFetcher,
  tokenRetryMs,
  wantedAccounts,
} from '../pushRules';

const NOTIFY = 'https://push.example.org/_matrix/push/v1/notify';

function registration(change: Partial<PusherRegistration> = {}): PusherRegistration {
  return { token: 'ExponentPushToken[a]', gateway: NOTIFY, lang: 'en', ...change };
}

describe('notifyUrl', () => {
  it('a gateway base url should get the notify path', () => {
    expect(notifyUrl('https://push.example.org/')).toBe(NOTIFY);
  });

  it('a full notify url should stay as it is', () => expect(notifyUrl(NOTIFY)).toBe(NOTIFY));

  it('no gateway should mean none', () => {
    expect(notifyUrl(undefined)).toBeNull();
    expect(notifyUrl('  ')).toBeNull();
  });

  it('a plain http gateway should only be taken on this machine', () => {
    expect(notifyUrl('http://push.example.org')).toBeNull();
    expect(notifyUrl('http://10.0.2.2:5000')).toBe('http://10.0.2.2:5000/_matrix/push/v1/notify');
  });

  it('something that is no url should mean none', () => expect(notifyUrl('push gateway')).toBeNull());
});

describe('unavailable', () => {
  it('a build without a gateway should say so', () => {
    expect(unavailable({ gateway: undefined, projectId: 'p' })).toBe('no-gateway');
  });

  it('a build without an eas project should say so', () => {
    expect(unavailable({ gateway: NOTIFY, projectId: undefined })).toBe('no-project');
  });

  it('a configured build should push', () => expect(unavailable({ gateway: NOTIFY, projectId: 'p' })).toBeNull());
});

describe('wantedAccounts', () => {
  it('every account should get pushes by default', () => {
    expect(wantedAccounts([ADA, GRACE], { pushEnabled: true, pushOff: [] })).toEqual([ADA, GRACE]);
  });

  it('an account turned off should get none', () => {
    expect(wantedAccounts([ADA, GRACE], { pushEnabled: true, pushOff: [GRACE] })).toEqual([ADA]);
  });

  it('push turned off on the device should leave no account', () => {
    expect(wantedAccounts([ADA, GRACE], { pushEnabled: false, pushOff: [] })).toEqual([]);
  });
});

describe('carryOut', () => {
  /** Steps over a remembered list, failing the server calls named in `failing`. */
  function steps(saved: Record<string, PusherRegistration>, failing: { remove?: boolean; register?: boolean } = {}) {
    const current = registration();
    const log: string[] = [];
    const s: PlanSteps = {
      signedIn: () => true,
      remove: async (account) => {
        log.push(`remove ${account}`);
        if (failing.remove) throw new Error('offline');
      },
      register: async (account) => {
        log.push(`register ${account}`);
        if (failing.register) throw new Error('offline');
      },
      removed: (account) => delete saved[account],
      registered: (account) => (saved[account] = current),
      failed: (account) => log.push(`failed ${account}`),
    };
    return { s, log, current };
  }

  it('a failed registration should be tried again', async () => {
    const saved = {};
    const { s, current } = steps(saved, { register: true });
    expect(await carryOut(plan(saved, [ADA], current), s)).toBe(false);
    expect(await carryOut(plan(saved, [ADA], current), steps(saved).s)).toBe(true);
    expect(saved).toEqual({ [ADA]: current });
  });

  it('a new token should not replace a pusher whose removal failed', async () => {
    const old = registration({ token: 'ExponentPushToken[old]' });
    const saved: Record<string, PusherRegistration> = { [ADA]: old };
    const { s, log, current } = steps(saved, { remove: true });
    expect(await carryOut(plan(saved, [ADA], current), s)).toBe(false);
    expect([saved[ADA], log]).toEqual([old, [`remove ${ADA}`]]);
  });

  it('a removal that went through on retry should let the new token register', async () => {
    const old = registration({ token: 'ExponentPushToken[old]' });
    const saved: Record<string, PusherRegistration> = { [ADA]: old };
    const { s, current } = steps(saved, { remove: true });
    await carryOut(plan(saved, [ADA], current), s);
    expect(await carryOut(plan(saved, [ADA], current), steps(saved).s)).toBe(true);
    expect(saved[ADA]).toEqual(current);
  });
});

describe('tokenFetcher', () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  it('a token that could not be had should be asked for again later', async () => {
    const get = jest.fn<Promise<string>, []>().mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce('ExponentPushToken[a]');
    const got = jest.fn();
    tokenFetcher({ get, got, wanted: () => true })();
    await jest.advanceTimersByTimeAsync(tokenRetryMs(1));
    expect([get.mock.calls.length, got.mock.calls]).toEqual([2, [['ExponentPushToken[a]']]]);
  });

  it('a token no longer needed should not be asked for again', async () => {
    const get = jest.fn<Promise<string>, []>().mockRejectedValue(new Error('offline'));
    tokenFetcher({ get, got: jest.fn(), wanted: () => false })();
    await jest.advanceTimersByTimeAsync(tokenRetryMs(1));
    expect(get).toHaveBeenCalledTimes(1);
  });

  it('the wait between asks should double up to ten minutes', () => {
    expect([1, 2, 3, 10].map(tokenRetryMs)).toEqual([30_000, 60_000, 120_000, 600_000]);
  });
});

describe('pushAllowed', () => {
  it('a push for an account turned off should not show', () => {
    expect(pushAllowed(GRACE, { pushEnabled: true, pushOff: [GRACE] })).toBe(false);
  });

  it('a push with push turned off on the device should not show', () => {
    expect(pushAllowed(ADA, { pushEnabled: false, pushOff: [] })).toBe(false);
  });

  it('a push for a wanted account should show', () => {
    expect(pushAllowed(ADA, { pushEnabled: true, pushOff: [GRACE] })).toBe(true);
  });
});

describe('plan', () => {
  it('a new account should be registered', () => {
    expect(plan({}, [ADA], registration())).toEqual({ register: [ADA], remove: [] });
  });

  it('an account registered as it is should be left alone', () => {
    expect(plan({ [ADA]: registration() }, [ADA], registration())).toEqual({ register: [], remove: [] });
  });

  it('a new token should replace the old pusher', () => {
    const old = registration({ token: 'ExponentPushToken[old]' });
    expect(plan({ [ADA]: old }, [ADA], registration())).toEqual({ register: [ADA], remove: [{ account: ADA, registration: old }] });
  });

  it('a new language should register again without removing', () => {
    expect(plan({ [ADA]: registration() }, [ADA], registration({ lang: 'de' }))).toEqual({ register: [ADA], remove: [] });
  });

  it('an account no longer wanted should lose its pusher', () => {
    const plain = registration();
    expect(plan({ [ADA]: plain }, [], plain)).toEqual({ register: [], remove: [{ account: ADA, registration: plain }] });
  });

  it('no token yet should keep what is registered', () => {
    expect(plan({ [ADA]: registration() }, [ADA], null)).toEqual({ register: [], remove: [] });
  });

  it('no token yet should still remove what is no longer wanted', () => {
    expect(plan({ [ADA]: registration() }, [], null).remove).toHaveLength(1);
  });
});

describe('pusherOf', () => {
  const fallback = { title: 'Better Matrix', body: 'New message' };

  it('an ios pusher should carry the fallback text for the extension', () => {
    const pusher = pusherOf(registration(), 'ios', 'iPhone', 'Better Matrix', fallback);
    expect(JSON.parse(pusher.payload ?? '{}')).toEqual({ alert: fallback });
  });

  it('an android pusher should carry data only', () => {
    const pusher = pusherOf(registration(), 'android', 'Pixel', 'Better Matrix', fallback);
    expect([pusher.app_id, pusher.push_key, pusher.gateway_url, pusher.payload]).toEqual([
      PUSH_APP_ID,
      'ExponentPushToken[a]',
      NOTIFY,
      null,
    ]);
  });
});

describe('targetOfPush', () => {
  const data = { account: ADA, room_id: '!r', event_id: '$e', unread: 3 };

  it('a push as android hands it over should name its account and event', () => {
    const json = JSON.stringify({ ...data, unread: '3' });
    expect(targetOfPush({ body: json, dataString: json, experienceId: '@o/better-matrix' })).toEqual({
      account: ADA,
      room_id: '!r',
      event_id: '$e',
      unread: 3,
    });
  });

  it('a push as ios hands it over should read its data from the body', () => {
    expect(targetOfPush({ aps: { alert: {} }, body: data })?.event_id).toBe('$e');
  });

  it('flat data should be read as it is', () => expect(targetOfPush(data)?.unread).toBe(3));

  it('a push without an event should name nothing', () => {
    expect(targetOfPush({ body: { account: ADA, unread: 0 } })).toBeNull();
  });

  it('a push of someone else should name nothing', () => {
    expect(targetOfPush({ body: '{"title":"hi"}' })).toBeNull();
    expect(targetOfPush(null)).toBeNull();
  });
});

describe('isPush', () => {
  it('a push should be told from what the app scheduled', () => {
    expect(isPush({ type: 'push', payload: {} })).toBe(true);
    expect(isPush({ type: 'channel', channelId: 'messages' })).toBe(false);
    expect(isPush(null)).toBe(false);
  });
});

describe('Seen', () => {
  it('an event should show once', () => {
    const seen = new Seen();
    expect([seen.mark(ADA, '$e'), seen.mark(ADA, '$e')]).toEqual([true, false]);
  });

  it('the same event of another account should show too', () => {
    const seen = new Seen();
    seen.mark(ADA, '$e');
    expect(seen.mark(GRACE, '$e')).toBe(true);
  });

  it('the oldest events should be forgotten past the limit', () => {
    const seen = new Seen(2);
    seen.mark(ADA, '$1');
    seen.mark(ADA, '$2');
    seen.mark(ADA, '$3');
    expect(seen.mark(ADA, '$1')).toBe(true);
  });
});

describe('leftToPush', () => {
  it('ios in the background should leave an account with a pusher to the extension', () => {
    expect(leftToPush(true, false, true)).toBe(true);
  });

  it('the app in front should notify itself', () => expect(leftToPush(true, true, true)).toBe(false));

  it('an account without a pusher should notify itself', () => expect(leftToPush(true, false, false)).toBe(false));

  it('android should notify itself and show each event once', () => expect(leftToPush(false, false, true)).toBe(false));
});

describe('badgeCount', () => {
  it('the badge should count conversations wanting attention over every account', () => {
    let state = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });
    state = apply(state, { type: 'AccountAdded', data: summary(GRACE, 'Grace') });
    const rooms = (account: string, unread: number[]) =>
      apply(state, {
        type: 'RoomsUpdated',
        data: { account, list: { rooms: unread.map((u, i) => ({ ...roomSummary(`!${i}`), unread: u })), spaces: [], invitations: [] } },
      });
    state = rooms(ADA, [2, 0]);
    state = rooms(GRACE, [1]);
    expect(badgeCount(state.accounts)).toBe(2);
  });
});
