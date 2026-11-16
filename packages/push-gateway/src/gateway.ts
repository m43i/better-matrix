// Better Matrix's push gateway: homeservers speak the Matrix Push Gateway API
// (https://spec.matrix.org/v1.16/push-gateway-api/), the app gets its pushes from Expo's push
// service, and this translates one into the other. It keeps nothing and logs no payload.
//
// Pushers use `format: "event_id_only"`, so a notification holds the room and event IDs and the
// unread count, never content: that is all Expo (and Apple or Google behind it) sees, with the
// pusher's `default_payload` (the account it is for, and on iOS a generic fallback text). The
// device fetches and decrypts the event itself before showing anything (ADR 0010).

import { z } from 'zod';

/** What the gateway needs to know. */
export interface GatewayConfig {
  /** The pusher app IDs it accepts; pushes for others are rejected. */
  appIds: readonly string[];
  /** An Expo access token, when the Expo project requires one for sending ("enhanced security"). */
  expoAccessToken?: string;
  /** Expo's send endpoint. */
  expoUrl?: string;
}

export const EXPO_SEND_URL = 'https://exp.host/--/api/v2/push/send';

/** The path homeservers post notifications to. */
export const NOTIFY_PATH = '/_matrix/push/v1/notify';

/** Expo takes at most this many messages per request. */
const EXPO_BATCH = 100;

const deviceSchema = z.object({
  app_id: z.string(),
  pushkey: z.string(),
  pushkey_ts: z.number().optional(),
  data: z.object({ default_payload: z.json().optional() }).optional(),
  tweaks: z.record(z.string(), z.json()).optional(),
});

/** One device of a notification, validated at the HTTP boundary. */
export type Device = z.infer<typeof deviceSchema>;

const notificationSchema = z.object({
  event_id: z.string().optional(),
  room_id: z.string().optional(),
  prio: z.enum(['high', 'low']).optional(),
  counts: z
    .object({ unread: z.number().optional(), missed_calls: z.number().optional() })
    .optional(),
  devices: z.array(deviceSchema),
});

export type MatrixNotification = z.infer<typeof notificationSchema>;

const payloadSchema = z.record(z.string(), z.json());

/** One message for Expo's push API. */
export interface ExpoMessage {
  to: string;
  data: z.infer<typeof payloadSchema>;
  title?: string;
  body?: string;
  sound?: 'default';
  /** iOS: lets the Notification Service Extension replace the text before it shows. */
  mutableContent?: boolean;
  priority: 'high' | 'normal';
}

/** Whether `token` is an Expo push token. */
export function isExpoToken(token: string): boolean {
  return /^Expo(nent)?PushToken\[[^\]]+\]$/.test(token);
}

/** Only the fallback title and body are shown; malformed fields are ignored. */
const alertSchema = z.object({
  title: z.string().optional().catch(undefined),
  body: z.string().optional().catch(undefined),
});

/**
 * The Expo messages for one notification, and the pushkeys to reject at once (an app ID this
 * gateway doesn't serve, a token that isn't Expo's). A notification without an event (a
 * badge-only update) sends nothing.
 *
 * Each message's data is the pusher's `default_payload` (minus `alert`) with `room_id`,
 * `event_id` and `unread`. With an `alert` (iOS) the message shows that text and is mutable, so
 * the Notification Service Extension can replace it; without one (Android) it is data only, so
 * the app's push task decides what shows.
 */
export function translate(notification: MatrixNotification, appIds: readonly string[]) {
  const messages: ExpoMessage[] = [];
  const rejected: string[] = [];

  for (const device of notification.devices) {
    if (!appIds.includes(device.app_id) || !isExpoToken(device.pushkey)) {
      rejected.push(device.pushkey);
      continue;
    }

    if (!notification.event_id || !notification.room_id) continue;
    const parsed = payloadSchema.safeParse(device.data?.default_payload);
    const payload = parsed.success ? parsed.data : {};
    const fallback = alertSchema.safeParse(payload.alert);
    const alert = fallback.success ? fallback.data : null;
    delete payload.alert;

    if (notification.counts?.unread !== undefined) payload.unread = notification.counts.unread;

    const data = {
      ...payload,
      room_id: notification.room_id,
      event_id: notification.event_id,
    };

    const priority = notification.prio === 'low' ? 'normal' : 'high';
    messages.push(
      alert
        ? {
            to: device.pushkey,
            data,
            title: alert.title,
            body: alert.body,
            sound: 'default',
            mutableContent: true,
            priority,
          }
        : { to: device.pushkey, data, priority },
    );
  }

  return { messages, rejected };
}

/** Expo's per-message tickets are decoded before deciding whether to discard a token. */
const ticketsSchema = z.object({
  data: z.array(
    z.object({
      status: z.enum(['ok', 'error']),
      message: z.string().optional(),
      details: z.object({ error: z.string().optional() }).optional(),
    }),
  ),
});

/** The HTTP dependency used by the gateway; no runtime-specific fetch utilities are needed. */
export type Fetch = (url: string, init: RequestInit) => Promise<Response>;

/** Expo refused the request as a whole, or couldn't be reached. */
export class ExpoUnavailable extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ExpoUnavailable';
  }
}

/** Ticket errors that pass: Expo would take the message later. */
const RETRIABLE = new Set(['MessageRateExceeded']);

/**
 * Sends `messages` to Expo, a batch at a time; the tokens Expo says no longer belong to an
 * installed app (`DeviceNotRegistered`), which the homeserver should drop. Throws
 * `ExpoUnavailable` when Expo refuses, can't be reached or asks to slow down
 * (`MessageRateExceeded`), so the homeserver tries again later; the app shows a push it gets
 * twice once.
 */
export async function send(
  messages: readonly ExpoMessage[],
  config: GatewayConfig,
  fetchFn: Fetch = fetch,
): Promise<string[]> {
  const gone: string[] = [];

  for (let start = 0; start < messages.length; start += EXPO_BATCH) {
    const batch = messages.slice(start, start + EXPO_BATCH);

    const headers = new Headers({
      'content-type': 'application/json',
      accept: 'application/json',
    });

    if (config.expoAccessToken) headers.set('authorization', `Bearer ${config.expoAccessToken}`);
    let response: Response;

    try {
      response = await fetchFn(config.expoUrl ?? EXPO_SEND_URL, {
        method: 'POST',
        headers,
        body: JSON.stringify(batch),
      });
    } catch (error) {
      throw new ExpoUnavailable(error instanceof Error ? error.message : String(error));
    }

    if (!response.ok) throw new ExpoUnavailable(`Expo answered ${response.status}`);
    const answer = ticketsSchema.safeParse(await response.json());

    if (!answer.success) throw new ExpoUnavailable('Expo returned invalid tickets');
    const tickets = answer.data.data;

    const retriable = tickets.find(
      (ticket) => ticket.status === 'error' && RETRIABLE.has(ticket.details?.error ?? ''),
    );

    if (retriable) throw new ExpoUnavailable(`Expo answered ${retriable.details?.error}`);
    tickets.forEach((ticket, index) => {
      const message = batch[index];

      if (message && ticket.status === 'error' && ticket.details?.error === 'DeviceNotRegistered')
        gone.push(message.to);
    });
  }

  return gone;
}

function json(
  status: number,
  body: { ok: true } | { errcode: string; error: string } | { rejected: string[] },
): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

/**
 * Handles one HTTP request: `POST /_matrix/push/v1/notify` translates and sends, answering the
 * pushkeys the homeserver should drop as `rejected`; `GET /health` says it runs.
 */
export async function handle(
  request: Request,
  config: GatewayConfig,
  fetchFn: Fetch = fetch,
): Promise<Response> {
  const path = new URL(request.url).pathname;

  if (path === '/health' && request.method === 'GET') return json(200, { ok: true });

  if (path !== NOTIFY_PATH)
    return json(404, { errcode: 'M_UNRECOGNIZED', error: 'Unknown endpoint' });

  if (request.method !== 'POST') return json(405, { errcode: 'M_UNRECOGNIZED', error: 'Use POST' });
  let body: unknown;

  try {
    body = await request.json();
  } catch {
    return json(400, { errcode: 'M_NOT_JSON', error: 'The body is not JSON' });
  }

  const parsed = z.object({ notification: notificationSchema }).safeParse(body);

  if (!parsed.success) {
    return json(400, { errcode: 'M_BAD_JSON', error: 'Expected a notification with devices' });
  }

  const { messages, rejected } = translate(parsed.data.notification, config.appIds);

  try {
    rejected.push(...(await send(messages, config, fetchFn)));
  } catch (error) {
    // The homeserver retries with backoff.
    const message = error instanceof Error ? error.message : String(error);

    return json(502, { errcode: 'M_UNKNOWN', error: message });
  }

  return json(200, { rejected });
}

/** The default app ID: the app registers its pushers under it (apps/mobile/src/engine/push). */
export const APP_ID = 'chat.bettermatrix.mobile';

/** Configuration from environment variables: `APP_IDS` (comma-separated), `EXPO_ACCESS_TOKEN`. */
export function configFrom(env: Record<string, string | undefined>): GatewayConfig {
  const appIds = (env.APP_IDS ?? APP_ID)
    .split(',')
    .map((id) => id.trim())
    .filter(Boolean);

  return { appIds, expoAccessToken: env.EXPO_ACCESS_TOKEN || undefined };
}
