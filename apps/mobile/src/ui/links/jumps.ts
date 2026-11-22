// Jumps to a message: the engine is asked which timeline shows it (`LocateMessage`), and its
// answer may come after the user moved on. Only the latest jump may still navigate, and only while
// the screen that asked for it is on show, so a late answer never takes the user from where they
// went since.

/** Whether a jump's answer may still navigate. */
export type Jump = () => boolean;

export interface Jumps {
  /**
   * Begins a jump asked for by a screen that is on show while `shown` says so; any jump begun
   * before it is superseded.
   */
  begin(shown?: () => boolean): Jump;
}

const always = () => true;

export function jumps(): Jumps {
  let latest = 0;

  return {
    begin(shown = always) {
      latest += 1;
      const ticket = latest;

      return () => ticket === latest && shown();
    },
  };
}
