/**
 * Letters for an avatar without an image, as `matrix_core::model::initials`: the first letter
 * of each of a display name's first two words, or of a Matrix ID's local part; `?` when blank.
 */
export function initials(name: string): string {
  let text = name;
  if (/^[@#!]/.test(text)) text = text.slice(1).split(':')[0] ?? '';
  const letters = text
    .split(/\s+/)
    .map((word) => [...word].find((c) => /[\p{L}\p{N}]/u.test(c)))
    .filter((c): c is string => c !== undefined)
    .slice(0, 2)
    .map((c) => c.toUpperCase())
    .join('');
  return letters || '?';
}
