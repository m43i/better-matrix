// How the Sessions page lists the account's sessions.

import type { Session } from '@better-matrix/matrix-native/types';

import { t } from '../../i18n';

/** This session first, then the others by when they were last active. */
export function sortSessions(sessions: Session[]): Session[] {
  return [...sessions].sort(
    (a, b) => Number(b.current) - Number(a.current) || (b.last_seen_ms ?? 0) - (a.last_seen_ms ?? 0),
  );
}

/** What a session's row says under its name: its ID, last activity and address. */
export function sessionDetails(session: Session, formatDate: (ms: number) => string): string {
  const details = [session.device_id];
  if (session.last_seen_ms !== null) details.push(t('sessions.last_seen', { when: formatDate(session.last_seen_ms) }));
  if (session.last_seen_ip) details.push(session.last_seen_ip);
  return details.join(' · ');
}
