import { DEFAULT_SETTINGS, parseSettings } from '../settings';

describe('parseSettings', () => {
  it('push should be on for every account by default', () => {
    expect([parseSettings(null).pushEnabled, parseSettings(null).pushOff]).toEqual([true, []]);
  });

  it('saved push choices should be read back', () => {
    const saved = parseSettings(
      JSON.stringify({ ...DEFAULT_SETTINGS, pushEnabled: false, pushOff: ['@a:x'] }),
    );

    expect([saved.pushEnabled, saved.pushOff]).toEqual([false, ['@a:x']]);
  });

  it('broken push choices should fall back to the defaults', () => {
    const saved = parseSettings(JSON.stringify({ pushEnabled: 'yes', pushOff: [1] }));
    expect([saved.pushEnabled, saved.pushOff]).toEqual([true, []]);
  });
});
