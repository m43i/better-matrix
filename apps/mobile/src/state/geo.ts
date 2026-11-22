// Places: `geo:` URIs (RFC 5870) read and written as the engine's `geo.rs` does, the map of a
// place as media (`osm:<lat>,<lon>,<zoom>`, which the engine draws from OpenStreetMap), and the
// place's page on openstreetmap.org. Every map shows OpenStreetMap's credit (`OSM_CREDIT`).

import type { MediaRef } from '@better-matrix/matrix-native/types';

/** How close maps are drawn (the engine's `MAP_ZOOM`). */
const MAP_ZOOM = 15;

/** The credit every map carries, as OpenStreetMap's licence asks; never translated. */
export const OSM_CREDIT = '© OpenStreetMap contributors';

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

  return `osm:${trimNumber(place.latitude)},${trimNumber(place.longitude)},${MAP_ZOOM}`;
}

/** The place on openstreetmap.org, to open in the browser. */
export function webLink(uri: string): string | null {
  const place = coordinates(uri);

  if (!place) return null;
  const latitude = trimNumber(place.latitude);
  const longitude = trimNumber(place.longitude);

  return `https://www.openstreetmap.org/?mlat=${latitude}&mlon=${longitude}#map=17/${latitude}/${longitude}`;
}

/** A place's coordinates as people read them: `52.5163, 13.3777`. */
export function coordinatesText(uri: string): string | null {
  const place = coordinates(uri);

  return place ? `${trimNumber(place.latitude)}, ${trimNumber(place.longitude)}` : null;
}
