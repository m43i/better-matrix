// The time lists are drawn at, moving on once a minute: rows show "today" and "yesterday" right
// without redrawing on every store change.

import { useEffect, useState } from 'react';

const MINUTE_MS = 60_000;

export function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), MINUTE_MS);
    return () => clearInterval(timer);
  }, []);
  return now;
}
