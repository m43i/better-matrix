// Permalinks: matrix.to links and `matrix:` URIs naming a user, a room or a message in one. The
// engine reads them in Rust (packages/matrix-core/src/permalinks.rs); the app reads them here too,
// because a link must be routed before the engine could answer. Both are tested against the same
// vectors (packages/matrix-core/src/permalinks.json), so they read every link the same way.

/** A parsed permalink, in the shape of the shared vectors. */
export interface Permalink {
  kind: 'user' | 'room';
  /** A user ID (`@…`), or a room ID (`!…`) or alias (`#…`). */
  id: string;
  /** For a room, the linked message's event ID (`$…`). */
  event: string | null;
  /** Servers to ask about the room. */
  via: string[];
  /** What a `matrix:` URI asks for (`?action=`); matrix.to links never ask. */
  action: 'join' | 'chat' | null;
}

/** Reads `link` as a matrix.to link or a `matrix:` URI; `null` for anything else. */
export function parsePermalink(link: string): Permalink | null {
  const trimmed = link.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '');
  const uri = stripPrefix(trimmed, 'matrix:');
  if (uri !== null) return matrixUri(uri);
  const web = stripPrefix(trimmed, 'https://') ?? stripPrefix(trimmed, 'http://');
  if (web === null) return null;
  const host = stripPrefix(web, 'matrix.to');
  if (host === null) return null;
  const rest = host.startsWith('/') ? host.slice(1) : host;
  if (!rest.startsWith('#')) return null;
  const fragment = rest.slice(1);
  return matrixTo(fragment.startsWith('/') ? fragment.slice(1) : fragment);
}

/** Whether `link` is a permalink, which opens in the app rather than the browser. */
export function isPermalink(link: string): boolean {
  return parsePermalink(link) !== null;
}

/** A matrix.to fragment after `#/`: `<id>[/<event>][?query]`. */
function matrixTo(fragment: string): Permalink | null {
  const [path, query] = splitOnce(fragment, '?');
  const segments = path.split('/').map(decode);
  if (segments.length > 2) return null;
  const id = segments[0];
  if (id === null || id === undefined) return null;
  let event: string | null = null;
  if (segments.length === 2) {
    const raw = segments[1];
    if (raw === null || raw === undefined || !isId(raw, '$', false)) return null;
    event = raw;
  }
  let kind: Permalink['kind'];
  if (isId(id, '@', true) && event === null) kind = 'user';
  else if (isId(id, '#', true) || isId(id, '!', false)) kind = 'room';
  else return null;
  const read = readQuery(query);
  return read && { kind, id, event, ...read };
}

/** A `matrix:` URI after the scheme: `<kind>/<id>[/e/<event>][?query][#fragment]`. */
function matrixUri(rest: string): Permalink | null {
  // The authority is reserved; nothing uses it yet.
  if (rest.startsWith('//')) return null;
  const [beforeFragment] = splitOnce(rest, '#');
  const [path, query] = splitOnce(beforeFragment, '?');
  const decoded = path.split('/').map(decode);
  if (decoded.some((segment) => segment === null)) return null;
  const segments = decoded as string[];
  const [type, value, e, event] = segments;
  if (value === undefined || !(segments.length === 2 || (segments.length === 4 && e === 'e'))) return null;
  let kind: Permalink['kind'];
  let id: string | null;
  if (type === 'u' && segments.length === 2) {
    kind = 'user';
    id = sigiled('@', value, true);
  } else if (type === 'r') {
    kind = 'room';
    id = sigiled('#', value, true);
  } else if (type === 'roomid') {
    kind = 'room';
    id = sigiled('!', value, false);
  } else return null;
  if (id === null) return null;
  let eventId: string | null = null;
  if (event !== undefined) {
    eventId = sigiled('$', event, false);
    if (eventId === null) return null;
  }
  const read = readQuery(query);
  return read && { kind, id, event: eventId, ...read };
}

/** `?via=…&action=…`: every non-empty `via`, and the action when this app knows it. */
function readQuery(query: string): Pick<Permalink, 'via' | 'action'> | null {
  const via: string[] = [];
  let action: Permalink['action'] = null;
  for (const pair of query.split('&')) {
    if (pair === '') continue;
    const [key, raw] = splitOnce(pair, '=');
    const value = decode(raw);
    if (value === null) return null;
    if (key === 'via' && value !== '') via.push(value);
    else if (key === 'action') action = value === 'join' || value === 'chat' ? value : null;
  }
  return { via, action };
}

function sigiled(sigil: string, id: string, needsServer: boolean): string | null {
  const full = sigil + id;
  return isId(full, sigil, needsServer) ? full : null;
}

/** A Matrix ID with `sigil`: something after it, no spaces or control characters, and a server when asked. */
function isId(id: string, sigil: string, needsServer: boolean): boolean {
  if (!id.startsWith(sigil)) return false;
  const rest = id.slice(sigil.length);
  if (rest === '' || /[\u0000- \u007f]/.test(rest)) return false;
  if (!needsServer) return true;
  const colon = rest.indexOf(':');
  return colon > 0 && colon < rest.length - 1;
}

/** Percent-decoding as the Rust does; `null` for a broken escape or bytes that aren't UTF-8. */
function decode(text: string): string | null {
  try {
    return decodeURIComponent(text);
  } catch {
    return null;
  }
}

/** `text` split at the first `separator`; the second part is empty without one. */
function splitOnce(text: string, separator: string): [string, string] {
  const at = text.indexOf(separator);
  return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + separator.length)];
}

/** `text` after `prefix` (lowercase ASCII), ignoring ASCII case as Rust's `eq_ignore_ascii_case` does. */
function stripPrefix(text: string, prefix: string): string | null {
  const head = text.slice(0, prefix.length).replace(/[A-Z]/g, (c) => c.toLowerCase());
  return head === prefix ? text.slice(prefix.length) : null;
}
