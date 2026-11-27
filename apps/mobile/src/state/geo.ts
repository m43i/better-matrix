import type {
  Location,
  MediaRef,
  Message,
  TimelineEntry,
} from '@better-matrix/matrix-native/types';

import { t } from '../i18n';
import { clockTime, sentAt } from './timelineText';

/** How close maps are drawn (the engine's `MAP_ZOOM`). */
const MAP_ZOOM = 15;

/** The latitude and longitude a `geo:` URI names, in degrees; `null` when it names none. */
export function coordinates(geoUri: string): { latitude: number; longitude: number } | null {
  const trimmed = geoUri.trim();

  if (trimmed.slice(0, 4).toLowerCase() !== 'geo:') return null;
  const rest = trimmed.slice(4);
  const semicolon = rest.indexOf(';');
  const point = semicolon < 0 ? rest : rest.slice(0, semicolon);
  const params = semicolon < 0 ? '' : rest.slice(semicolon + 1);

  // Only WGS-84, the default, maps onto OpenStreetMap.
  const crs = params
    .split(';')
    .map((param) => /^crs=(.*)$/i.exec(param)?.[1])
    .find((value) => value !== undefined);

  if (crs !== undefined && crs.toLowerCase() !== 'wgs84') return null;
  const [lat, lon] = point.split(',');
  const latitude = parseNumber(lat);
  const longitude = parseNumber(lon);

  if (latitude === null || longitude === null) return null;

  if (Math.abs(latitude) > 90 || Math.abs(longitude) > 180) return null;

  return { latitude, longitude };
}

function parseNumber(text: string | undefined): number | null {
  const trimmed = text?.trim() ?? '';

  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(trimmed)) return null;
  const value = Number(trimmed);

  return Number.isFinite(value) ? value : null;
}

/** `value` to six decimals (about 10 cm), without trailing zeros. */
export function trimNumber(value: number): string {
  const text = value.toFixed(6).replace(/0+$/, '').replace(/\.$/, '');

  return text === '-0' ? '0' : text;
}

/** A `geo:` URI for a place, with its accuracy in metres when known. */
export function geoUri(latitude: number, longitude: number, accuracy?: number | null): string {
  const uri = `geo:${trimNumber(latitude)},${trimNumber(longitude)}`;

  return accuracy && accuracy > 0 ? `${uri};u=${Math.round(accuracy)}` : uri;
}

/** The map of a place, to fetch like any media; `null` when the URI names no place. */
export function mapOf(uri: string): MediaRef | null {
  const place = coordinates(uri);

  if (!place) return null;

  return `map:${trimNumber(place.latitude)},${trimNumber(place.longitude)},${MAP_ZOOM},light`;
}

/** A validated destination for the platform's navigation handler. */
export function navigationLink(uri: string, platform: string): string | null {
  const place = coordinates(uri);

  if (!place) return null;
  const point = `${trimNumber(place.latitude)},${trimNumber(place.longitude)}`;

  if (platform === 'android') return `geo:0,0?q=${point}`;

  if (platform === 'ios') return `geo-navigation:///place?coordinate=${point}`;

  return `https://www.google.com/maps/search/?api=1&query=${point}`;
}

/** A place's coordinates as people read them: `52.5163, 13.3777`. */
export function coordinatesText(uri: string): string | null {
  const place = coordinates(uri);

  return place ? `${trimNumber(place.latitude)}, ${trimNumber(place.longitude)}` : null;
}

/** How long a place someone shared stays on a conversation's map once it stops being live. */
export const RECENT_PLACE_MS = 24 * 60 * 60 * 1000;

/** One person's place on a conversation's map. */
export interface MapPin {
  /** The location message, whose sender the pin shows. */
  message: Message;
  location: Location;
  latitude: number;
  longitude: number;
}

/** A live share that hasn't ended at `now`. */
export function isLive(location: Location, now: number): boolean {
  return location.live_until != null && location.live_until > now;
}

/**
 * The pins a conversation's map shows (the desktop's `map_pins`): of the loaded location
 * messages with coordinates, each sender's newest, kept while it is live or under a day old. The
 * focused message is always shown, in place of its sender's newest. Newest first.
 */
export function mapPins(
  entries: readonly TimelineEntry[],
  focusEventId: string,
  now: number,
): MapPin[] {
  const newest = new Map<string, MapPin>();
  let focus: MapPin | null = null;

  for (const entry of entries) {
    if (entry.kind.type !== 'Message') continue;
    const message = entry.kind.data;

    if (message.content.type !== 'Location') continue;
    const location = message.content.data;
    const point = coordinates(location.geo_uri);

    if (!point) continue;
    const pin = { message, location, ...point };

    if (message.event_id === focusEventId) focus = pin;
    const held = newest.get(message.sender_id);

    if (!held || held.message.timestamp_ms < message.timestamp_ms)
      newest.set(message.sender_id, pin);
  }

  if (focus) newest.set(focus.message.sender_id, focus);

  return [...newest.values()]
    .filter(
      (pin) =>
        pin === focus ||
        isLive(pin.location, now) ||
        now - pin.message.timestamp_ms <= RECENT_PLACE_MS,
    )
    .sort((a, b) => b.message.timestamp_ms - a.message.timestamp_ms);
}

/** What a pin's row says under the name: until when it is live, else where and when it was shared. */
export function pinDetail(pin: MapPin, now: number): string {
  const until = pin.location.live_until;

  if (until != null && until > now) {
    // A share until stopped ends weeks on: name the day when it isn't today.
    const today = new Date(until).toDateString() === new Date(now).toDateString();

    return t('location.live_until', { time: today ? clockTime(until) : sentAt(until, now) });
  }

  const when = sentAt(pin.message.timestamp_ms, now);

  return pin.location.description ? `${pin.location.description} · ${when}` : when;
}
