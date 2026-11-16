// Permalinks the system opened the app with (`matrix:` URIs, matrix.to links on Android). They
// arrive before the router (app/+native-intent.ts), often before the accounts are back, so they
// wait here until `LinkOpener` can show them.

type Listener = () => void;

let waiting: string[] = [];
const listeners = new Set<Listener>();

/** Keeps `link` for the opener and tells it one came. */
export function deliverLink(link: string): void {
  waiting = [...waiting, link];
  for (const listener of listeners) listener();
}

/** The links waiting, oldest first; they are no longer kept. */
export function takeLinks(): string[] {
  const taken = waiting;
  waiting = [];
  return taken;
}

/** Hears every link delivered from now on; returns the unsubscribe. */
export function onLinkDelivered(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
