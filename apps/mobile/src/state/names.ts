import type { RoomSummary } from '@better-matrix/matrix-native/types';

/** Format a person's name for display while keeping the Matrix profile and user ID intact. */
export function personName(name: string, reverse: boolean): string {
  if (!reverse || (name.startsWith('@') && name.includes(':'))) return name;

  const comma = name.indexOf(',');

  if (comma < 0 || comma !== name.lastIndexOf(',')) return name;

  const last = name.slice(0, comma).trim();
  const first = name.slice(comma + 1).trim();

  return last && first ? `${first}, ${last}` : name;
}

/** A direct chat can take its name from the other person. Explicit room names are kept as typed. */
export function roomName(room: RoomSummary, reverse: boolean): string {
  return personName(room.name, reverse && room.name_from_member);
}

/** Keep a mention's leading @ with the displayed name. */
export function mentionName(label: string, reverse: boolean): string {
  if (!label.startsWith('@')) return personName(label, reverse);

  if (label.includes(':')) return label;

  return `@${personName(label.slice(1), reverse)}`;
}
