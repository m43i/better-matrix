// Where a room message's read indicator goes: in a column every message keeps free at its right,
// so a receipt never rewraps or grows its message, or, on a screen too narrow to spare that
// column, on a line under the message (as the desktop does in a cramped timeline).

import { createContext, useContext } from 'react';
import { useWindowDimensions } from 'react-native';

import { READER_FACES } from '../../state/readers';
import { font } from '../../theme/palette';
import { useTimelineContext } from './context';

/** A reader's face. */
export const FACE = 16;

/** The ring that separates overlapping faces; it adds to each face's size. */
export const FACE_BORDER = 1.5;

/** How much each face overlaps the one before. */
export const FACE_OVERLAP = 6;

const FACE_OUTER = FACE + 2 * FACE_BORDER;

/** Room for the overflow count ("+99", or "99+") after the faces. */
const COUNT = 24;

/** The indicator's height: a line of text. */
export const READERS_HEIGHT = font.body.lineHeight;

/** Padding above and below the faces that keeps the indicator to a line of text. */
export const READERS_PADDING_Y = (READERS_HEIGHT - FACE_OUTER) / 2;

/** The width of the readers' column. */
export const READERS_COLUMN =
  FACE_OUTER + (READER_FACES - 1) * (FACE_OUTER - FACE_OVERLAP) + 6 + COUNT + 8;

/** Room beside a message's content: the row's padding, the sender's picture and its gap. */
const ROW_INSET = 16 + 36 + 10 + 16;

/** The column with the gap before it. */
const READERS_INSET = READERS_COLUMN + 10;

/** The least width a message keeps beside the readers' column. */
export const MIN_CONTENT = 200;

/** Whether a screen `width` wide keeps the readers' column beside its messages. */
export function keepsReadersColumn(width: number): boolean {
  return width - ROW_INSET - READERS_INSET >= MIN_CONTENT;
}

/** Whether this timeline's messages keep the readers' column: a room's, on a wide enough screen. */
export function useReadersColumn(): boolean {
  const { width } = useWindowDimensions();
  const { thread } = useTimelineContext();

  return thread === null && keepsReadersColumn(width);
}

/** A narrower width for what lies inside it: a chat bubble's, for what the bubble holds. */
export const ContentWidthContext = createContext<number | null>(null);

/** The widest a message's content can be in this timeline, or inside its bubble. */
export function useContentWidth(): number {
  const { width } = useWindowDimensions();
  const inside = useContext(ContentWidthContext);
  const row = width - ROW_INSET - (useReadersColumn() ? READERS_INSET : 0);

  return inside ?? row;
}
