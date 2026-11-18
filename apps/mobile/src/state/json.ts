// Values as the bridge's JSON carries them, and comparing them by content: an event that repeats
// what the store holds keeps the store's objects, so views drawn from them don't redraw.

/** A JSON value: what engine events are made of. */
export type Json = null | boolean | number | string | readonly Json[] | JsonObject;

/** A JSON object. */
export interface JsonObject {
  readonly [key: string]: Json;
}

function isList(value: Json): value is readonly Json[] {
  return Array.isArray(value);
}

function isObject(value: Json): value is JsonObject {
  return value instanceof Object && !Array.isArray(value);
}

/** Whether two JSON values hold the same content. */
export function sameJson(a: Json, b: Json): boolean {
  if (a === b) return true;

  if (isList(a)) {
    if (!isList(b) || a.length !== b.length) return false;

    for (let i = 0; i < a.length; i++) {
      const left = a[i];
      const right = b[i];

      if (left === undefined || right === undefined || !sameJson(left, right)) return false;
    }

    return true;
  }

  if (!isObject(a) || !isObject(b)) return false;
  const keys = Object.keys(a);

  if (keys.length !== Object.keys(b).length) return false;

  for (const key of keys) {
    const left = a[key];
    const right = b[key];

    if (left === undefined || right === undefined || !sameJson(left, right)) return false;
  }

  return true;
}

/**
 * `next` with each item that has the same content as `previous`'s item of the same room kept as
 * `previous`'s object; `previous` itself when nothing changed.
 */
export function keepUnchanged<T extends Json & { room_id: string }>(previous: T[], next: T[]): T[] {
  if (previous.length === 0) return next.length === 0 ? previous : next;
  const before = new Map(previous.map((item) => [item.room_id, item]));
  let changed = previous.length !== next.length;

  const kept = next.map((item, i) => {
    const old = before.get(item.room_id);

    if (old === undefined || !sameJson(old, item)) {
      changed = true;

      return item;
    }

    if (previous[i] !== old) changed = true;

    return old;
  });

  return changed ? kept : previous;
}
