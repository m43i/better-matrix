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

  it('a saved playback speed should be read back', () => {
    expect(
      parseSettings(JSON.stringify({ ...DEFAULT_SETTINGS, playbackSpeed: 1.5 })).playbackSpeed,
    ).toBe(1.5);
  });

  it('an unknown playback speed should play as recorded', () => {
    expect(parseSettings(JSON.stringify({ playbackSpeed: 3 })).playbackSpeed).toBe(1);
  });

  it('maps from OpenStreetMap should be on by default', () => {
    expect(parseSettings(JSON.stringify({ language: 'en' })).showMaps).toBe(true);
  });

  it('maps turned off should stay off', () => {
    expect(parseSettings(JSON.stringify({ ...DEFAULT_SETTINGS, showMaps: false })).showMaps).toBe(
      false,
    );
  });

  it('messages should lay out in classic rows by default', () => {
    expect(parseSettings(null).messageLayout).toBe('rows');
  });

  it('a saved message layout should be read back', () => {
    expect(
      parseSettings(JSON.stringify({ ...DEFAULT_SETTINGS, messageLayout: 'bubbles' }))
        .messageLayout,
    ).toBe('bubbles');
  });

  it('an unknown message layout should show classic rows', () => {
    expect(parseSettings(JSON.stringify({ messageLayout: 'cards' })).messageLayout).toBe('rows');
  });

  it('recently sent stickers should be read back by account', () => {
    const SIDE: [number, number] = [256, 256];

    const sticker = {
      shortcode: 'wave',
      body: 'Waving hello',
      url: 'mxc://x/wave',
      media: 'm-wave',
      emoticon: false,
      sticker: true,
      dimensions: SIDE,
      mimetype: null,
    };

    const saved = JSON.stringify({ ...DEFAULT_SETTINGS, recentStickers: { '@ada:x': [sticker] } });

    expect(parseSettings(saved).recentStickers).toEqual({ '@ada:x': [sticker] });
  });

  it('broken recent stickers should be forgotten', () => {
    expect(
      parseSettings(JSON.stringify({ recentStickers: { '@ada:x': [1] } })).recentStickers,
    ).toEqual({});
  });
});
