// Typing notices from one composer, as the desktop's `state/typing.rs` decides them: `true` as
// the draft changes (again only once the last notice is about to lapse), `false` only where
// typing visibly ends (the draft emptied or sent, the composer left). Opening a composer with a
// draft in it isn't typing; changing it is.

/** How often a draft that keeps changing says so again: just under the SDK's notice lifetime. */
export const REPEAT_MS = 3000;

export interface TypingNotices {
  /** The draft as last seen. */
  draft: string;
  /** When the room was last told the user is typing; `null` once told they stopped. */
  told: number | null;
}

/** The notice to send, if any, and the tracker after it. */
export interface TypingStep {
  next: TypingNotices;
  notice: boolean | null;
}

/** A tracker for a composer opened with `draft` in it. */
export function typingFor(draft: string): TypingNotices {
  return { draft, told: null };
}

/** The draft changed to `draft` at `now` (ms). */
export function draftChanged(state: TypingNotices, draft: string, now: number): TypingStep {
  if (draft === state.draft) return { next: state, notice: null };
  if (draft.trim() === '') {
    return { next: { draft, told: null }, notice: state.told === null ? null : false };
  }
  if (state.told === null || now - state.told >= REPEAT_MS) return { next: { draft, told: now }, notice: true };
  return { next: { ...state, draft }, notice: null };
}

/** Typing ended without the draft emptying: sent, or the composer left the screen. */
export function typingEnded(state: TypingNotices, draft: string = state.draft): TypingStep {
  return { next: { draft, told: null }, notice: state.told === null ? null : false };
}
