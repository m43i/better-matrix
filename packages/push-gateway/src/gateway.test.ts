import { describe, expect, it } from 'bun:test';

import { configFrom, type Device, type ExpoMessage, ExpoUnavailable, handle, type MatrixNotification, NOTIFY_PATH, send, translate } from './gateway';

const TOKEN = 'ExponentPushToken[abc]';
const APP = 'chat.bettermatrix.mobile';
const config = { appIds: [APP] };

function device(change: Partial<Device> = {}): Device {
  return { app_id: APP, pushkey: TOKEN, data: { default_payload: { account: '@ada:example.org' } }, ...change };
}

function notification(change: Partial<MatrixNotification> = {}): MatrixNotification {
  return { event_id: '$e', room_id: '!r:example.org', prio: 'high', counts: { unread: 2 }, devices: [device()], ...change };
}

function notify(body: unknown): Request {
  return new Request(`https://push.example.org${NOTIFY_PATH}`, { method: 'POST', body: JSON.stringify(body) });
}

/** A fake Expo: answers each message with the ticket `ticket` gives, and records what it got. */
function expo(ticket: (message: ExpoMessage) => object = () => ({ status: 'ok', id: 'x' }), status = 200) {
  const sent: Array<{ headers: Headers; messages: ExpoMessage[] }> = [];
  const fetchFn = (async (_url: string, init: RequestInit) => {
    const messages = JSON.parse(String(init.body)) as ExpoMessage[];
    sent.push({ headers: new Headers(init.headers), messages });
    return new Response(JSON.stringify({ data: messages.map(ticket) }), { status });
  }) as unknown as typeof fetch;
  return { fetchFn, sent };
}

describe('translate', () => {
  it('an android push should carry the account, room, event and count as data only', () => {
    const { messages } = translate(notification(), [APP]);
    expect(messages).toEqual([
      {
        to: TOKEN,
        data: { account: '@ada:example.org', room_id: '!r:example.org', event_id: '$e', unread: 2 },
        priority: 'high',
      },
    ]);
  });

  it('an ios push should show its fallback text and let the extension replace it', () => {
    const payload = { account: '@ada:example.org', alert: { title: 'Better Matrix', body: 'New message' } };
    const [message] = translate(notification({ devices: [device({ data: { default_payload: payload } })] }), [APP]).messages;
    expect(message).toMatchObject({ title: 'Better Matrix', body: 'New message', mutableContent: true, sound: 'default' });
    expect(message?.data).toEqual({ account: '@ada:example.org', room_id: '!r:example.org', event_id: '$e', unread: 2 });
  });

  it('a push of low priority should not wake the device at once', () => {
    expect(translate(notification({ prio: 'low' }), [APP]).messages[0]?.priority).toBe('normal');
  });

  it('a badge-only update should send nothing', () => {
    const { messages, rejected } = translate(notification({ event_id: undefined }), [APP]);
    expect([messages, rejected]).toEqual([[], []]);
  });

  it('a pusher of another app should be rejected', () => {
    expect(translate(notification({ devices: [device({ app_id: 'org.other' })] }), [APP]).rejected).toEqual([TOKEN]);
  });

  it('a token that is not an expo push token should be rejected', () => {
    expect(translate(notification({ devices: [device({ pushkey: 'fcm-token' })] }), [APP]).rejected).toEqual(['fcm-token']);
  });

  it('a notification should never forward content the homeserver sent anyway', () => {
    const leaky = { ...notification(), content: { body: 'secret' }, sender_display_name: 'Grace' } as MatrixNotification;
    expect(JSON.stringify(translate(leaky, [APP]).messages)).not.toContain('secret');
  });
});

describe('send', () => {
  it('a token expo no longer knows should be dropped', async () => {
    const { fetchFn } = expo((m) =>
      m.to === TOKEN ? { status: 'error', message: 'gone', details: { error: 'DeviceNotRegistered' } } : { status: 'ok' },
    );
    const messages = translate(notification({ devices: [device(), device({ pushkey: 'ExpoPushToken[other]' })] }), [APP]).messages;
    expect(await send(messages, config, fetchFn)).toEqual([TOKEN]);
  });

  it('other expo errors should keep the token', async () => {
    const { fetchFn } = expo(() => ({ status: 'error', details: { error: 'MessageTooBig' } }));
    expect(await send(translate(notification(), [APP]).messages, config, fetchFn)).toEqual([]);
  });

  it('expo asking to slow down should fail the send', async () => {
    const { fetchFn } = expo(() => ({ status: 'error', details: { error: 'MessageRateExceeded' } }));
    await expect(send(translate(notification(), [APP]).messages, config, fetchFn)).rejects.toThrow(ExpoUnavailable);
  });

  it('more than a hundred messages should go in batches', async () => {
    const devices = Array.from({ length: 150 }, (_, i) => device({ pushkey: `ExponentPushToken[${i}]` }));
    const { fetchFn, sent } = expo();
    await send(translate(notification({ devices }), [APP]).messages, config, fetchFn);
    expect(sent.map((request) => request.messages.length)).toEqual([100, 50]);
  });

  it('an access token should be sent when the project requires one', async () => {
    const { fetchFn, sent } = expo();
    await send(translate(notification(), [APP]).messages, { ...config, expoAccessToken: 'secret' }, fetchFn);
    expect(sent[0]?.headers.get('authorization')).toBe('Bearer secret');
  });
});

describe('handle', () => {
  it('a notification should answer the tokens to drop', async () => {
    const { fetchFn } = expo(() => ({ status: 'error', details: { error: 'DeviceNotRegistered' } }));
    const response = await handle(notify({ notification: notification() }), config, fetchFn);
    expect([response.status, await response.json()]).toEqual([200, { rejected: [TOKEN] }]);
  });

  it('a delivered notification should reject nothing', async () => {
    const response = await handle(notify({ notification: notification() }), config, expo().fetchFn);
    expect(await response.json()).toEqual({ rejected: [] });
  });

  it('expo being down should ask the homeserver to try again', async () => {
    const response = await handle(notify({ notification: notification() }), config, expo(undefined, 503).fetchFn);
    expect(response.status).toBe(502);
  });

  it('expo asking to slow down should ask the homeserver to try again', async () => {
    const { fetchFn } = expo(() => ({ status: 'error', details: { error: 'MessageRateExceeded' } }));
    const response = await handle(notify({ notification: notification() }), config, fetchFn);
    expect(response.status).toBe(502);
  });

  it('a body that is not json should be refused', async () => {
    const request = new Request(`https://push.example.org${NOTIFY_PATH}`, { method: 'POST', body: '{' });
    expect((await handle(request, config, expo().fetchFn)).status).toBe(400);
  });

  it('a body without devices should be refused', async () => {
    const response = await handle(notify({ notification: { event_id: '$e' } }), config, expo().fetchFn);
    expect([response.status, (await response.json()).errcode]).toEqual([400, 'M_BAD_JSON']);
  });

  it('another path should be unknown', async () => {
    const response = await handle(new Request('https://push.example.org/other'), config, expo().fetchFn);
    expect(response.status).toBe(404);
  });
});

describe('configFrom', () => {
  it('the app id should default to the app', () => expect(configFrom({}).appIds).toEqual([APP]));

  it('several app ids should be read from a list', () => {
    expect(configFrom({ APP_IDS: 'a, b' }).appIds).toEqual(['a', 'b']);
  });
});
