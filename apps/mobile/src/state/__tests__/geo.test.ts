import type { Message, MessageContent, TimelineEntry } from '@better-matrix/matrix-native/types';

import {
  coordinates,
  coordinatesText,
  geoUri,
  mapOf,
  mapPins,
  pinDetail,
  googleMapsLink,
  RECENT_PLACE_MS,
} from '../geo';
import { clockTime, sentAt } from '../timelineText';

// The engine's `geo.rs` reads and writes the same (its doc tests).
describe('coordinates', () => {
  it('should read a geo URI with an accuracy', () => {
    expect(coordinates('geo:51.5008,0.1247;u=35')).toEqual({
      latitude: 51.5008,
      longitude: 0.1247,
    });
  });

  it('should read nothing from another address', () => {
    expect(coordinates('https://example.org')).toBeNull();
  });

  it('should read nothing in another coordinate system', () => {
    expect(coordinates('geo:1,2;crs=mars')).toBeNull();
  });

  it('should read nothing out of range', () => {
    expect(coordinates('geo:91,0')).toBeNull();
  });
});

describe('geoUri', () => {
  it('should write six decimals without trailing zeros', () => {
    expect(geoUri(52.516275, 13.3777)).toBe('geo:52.516275,13.3777');
  });

  it('should add how accurate the position is', () => {
    expect(geoUri(1, 2, 12.4)).toBe('geo:1,2;u=12');
  });
});

describe('mapOf', () => {
  it('should name the map the engine draws', () => {
    expect(mapOf('geo:52.5163,13.3777;u=10')).toBe('map:52.5163,13.3777,15,light');
  });
});

describe('googleMapsLink', () => {
  it('should search Google Maps for the coordinates', () => {
    expect(googleMapsLink('geo:52.5,13.4;u=10')).toBe(
      'https://www.google.com/maps/search/?api=1&query=52.5,13.4',
    );
  });

  it('should name nothing for another address', () => {
    expect(googleMapsLink('https://example.org')).toBeNull();
  });
});

describe('coordinatesText', () => {
  it('should show the coordinates as people read them', () => {
    expect(coordinatesText('geo:-0.0000001,2')).toBe('0, 2');
  });
});

describe('mapPins', () => {
  const NOW = 1_800_000_000_000;
  const HOUR = 3_600_000;

  const place = (
    id: string,
    sender: string,
    at: number,
    geo = 'geo:52.5,13.4',
    liveUntil?: number,
  ): TimelineEntry => {
    const content: MessageContent = {
      type: 'Location',
      data: { geo_uri: geo, description: null, map: null, live_until: liveUntil },
    };

    const message: Message = {
      event_id: id,
      sender_id: sender,
      sender_name: sender,
      sender_avatar: null,
      content,
      edited: false,
      editable: false,
      pending: false,
      timestamp_ms: at,
      reactions: [],
      reply_to: null,
      thread: null,
      highlighted: false,
    };

    return { id, kind: { type: 'Message', data: message } };
  };

  const ids = (entries: TimelineEntry[], focus = '$none') =>
    mapPins(entries, focus, NOW).map((pin) => pin.message.event_id);

  it('should keep only the newest place of each sender', () => {
    expect(ids([place('$old', '@a', NOW - 2 * HOUR), place('$new', '@a', NOW - HOUR)])).toEqual([
      '$new',
    ]);
  });

  it('should list the pins newest first', () => {
    expect(ids([place('$b', '@b', NOW - HOUR), place('$a', '@a', NOW - 3 * HOUR)])).toEqual([
      '$b',
      '$a',
    ]);
  });

  it('should leave out a place shared more than a day ago', () => {
    expect(ids([place('$old', '@a', NOW - RECENT_PLACE_MS - 1)])).toEqual([]);
  });

  it('should keep an old live share that is still live', () => {
    expect(ids([place('$live', '@a', NOW - 2 * RECENT_PLACE_MS, 'geo:1,2', NOW + HOUR)])).toEqual([
      '$live',
    ]);
  });

  it('should leave out an old live share that has ended', () => {
    expect(ids([place('$ended', '@a', NOW - 2 * RECENT_PLACE_MS, 'geo:1,2', NOW - 1)])).toEqual([]);
  });

  it('should leave out a live share that ended today, stopped or run out', () => {
    expect(
      ids([
        place('$expired', '@a', NOW - 2 * HOUR, 'geo:1,2', NOW - HOUR),
        place('$stopped', '@b', NOW - 10 * 60_000, 'geo:1,2', 0),
      ]),
    ).toEqual([]);
  });

  it('should leave out the places of people the user ignores', () => {
    const pins = mapPins([place('$a', '@a', NOW), place('$b', '@b', NOW)], '$a', NOW, ['@b']);
    expect(pins.map((pin) => pin.message.event_id)).toEqual(['$a']);
  });

  it('should always show the focused place, however old', () => {
    expect(ids([place('$old', '@a', NOW - 3 * RECENT_PLACE_MS)], '$old')).toEqual(['$old']);
  });

  it('should show the focused place in place of its sender’s newest', () => {
    expect(
      ids([place('$focus', '@a', NOW - 2 * HOUR), place('$newer', '@a', NOW - HOUR)], '$focus'),
    ).toEqual(['$focus']);
  });

  it('should leave out places without coordinates', () => {
    expect(ids([place('$waiting', '@a', NOW - HOUR, '')])).toEqual([]);
  });

  it('should read the pin’s coordinates from its geo URI', () => {
    const [pin] = mapPins([place('$a', '@a', NOW, 'geo:52.52,13.38;u=5')], '$a', NOW);
    expect([pin?.latitude, pin?.longitude]).toEqual([52.52, 13.38]);
  });

  it('should say until when a live share runs', () => {
    const until = NOW + HOUR;
    const [pin] = mapPins([place('$live', '@a', NOW, 'geo:1,2', until)], '$live', NOW);
    expect(pin && pinDetail(pin, NOW)).toBe(`Live until ${clockTime(until)}`);
  });

  it('should name the day a live share ends when it is not today', () => {
    const until = NOW + 20 * 24 * HOUR;
    const [pin] = mapPins([place('$live', '@a', NOW, 'geo:1,2', until)], '$live', NOW);
    expect(pin && pinDetail(pin, NOW)).toBe(`Live until ${sentAt(until, NOW)}`);
  });

  it('should say when a place that is not live was shared', () => {
    const [pin] = mapPins([place('$a', '@a', NOW - HOUR)], '$a', NOW);
    expect(pin && pinDetail(pin, NOW)).toBe(sentAt(NOW - HOUR, NOW));
  });
});
