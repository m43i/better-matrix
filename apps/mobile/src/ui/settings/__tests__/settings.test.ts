import type { Session } from '@better-matrix/matrix-native/types';

import { t } from '../../../i18n';
import { languageChoices } from '../choices';
import { sessionDetails, sortSessions } from '../sessions';

function session(device_id: string, last_seen_ms: number | null, current = false): Session {
  return { device_id, name: null, last_seen_ms, last_seen_ip: null, current, verified: true };
}

describe('languageChoices', () => {
  it('should offer the system language first', () => {
    expect(languageChoices()[0]!.id).toBe('system');
  });

  it('should offer every catalog after it', () => {
    expect(languageChoices().map((c) => c.id)).toEqual(['system', 'en', 'de', 'fr']);
  });
});

describe('sortSessions', () => {
  it('should list this session first, then the most recently active', () => {
    const sorted = sortSessions([session('OLD', 1), session('ME', 2, true), session('NEW', 3)]);
    expect(sorted.map((s) => s.device_id)).toEqual(['ME', 'NEW', 'OLD']);
  });
});

describe('sessionDetails', () => {
  it('should give the ID, the last activity and the address', () => {
    const details = sessionDetails({ ...session('DEV', 5), last_seen_ip: '10.0.0.1' }, () => 'today');
    expect(details).toBe(`DEV · ${t('sessions.last_seen', { when: 'today' })} · 10.0.0.1`);
  });

  it('should give only the ID of a session never seen', () => {
    expect(sessionDetails(session('DEV', null), () => 'today')).toBe('DEV');
  });
});
