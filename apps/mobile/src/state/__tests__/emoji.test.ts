import { emojiByGroup, GROUPS, RECENT_KEPT, rememberEmoji, RESULTS, searchEmoji } from '../emoji';

const found = (query: string) => searchEmoji(query).map((e) => e.emoji);

describe('searchEmoji', () => {
  it('should find an emoji by its name', () => {
    expect(found('grinning face')).toContain('😀');
  });

  it('should find an emoji by its shortcode, colons and all', () => {
    expect(found(':tada:')[0]).toBe('🎉');
  });

  it('should put an exact shortcode first', () => {
    expect(found('+1')[0]).toBe('👍');
  });

  it('should need every word to match', () => {
    expect(found('red heart')).toContain('❤️');
    expect(found('red heart')).not.toContain('💙');
  });

  it('should find nothing for an empty query', () => {
    expect(found('  ')).toEqual([]);
  });

  it('should cap the results', () => {
    expect(found('a').length).toBe(RESULTS);
  });
});

describe('emojiByGroup', () => {
  it('should list every group in order', () => {
    expect(emojiByGroup().map((g) => g.group)).toEqual([...GROUPS]);
  });

  it('should leave skin tone components out', () => {
    const all = emojiByGroup().flatMap((g) => g.emoji.map((e) => e.emoji));
    expect(all).not.toContain('🏻');
  });

  it('should start the smileys with the grinning face', () => {
    expect(emojiByGroup()[0]?.emoji[0]?.emoji).toBe('😀');
  });
});

describe('rememberEmoji', () => {
  it('should put the emoji used last first', () => {
    expect(rememberEmoji(['👍', '🎉'], '🎉')).toEqual(['🎉', '👍']);
  });

  it('should keep a limited number', () => {
    const many = Array.from({ length: RECENT_KEPT }, (_, i) => String(i));
    expect(rememberEmoji(many, 'new')).toHaveLength(RECENT_KEPT);
  });
});
