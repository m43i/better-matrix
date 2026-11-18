import { Lru } from '../lru';

describe('Lru', () => {
  it('should give back what it keeps', () => {
    const cache = new Lru<string, number>(2);
    cache.set('a', 1);

    expect(cache.get('a')).toBe(1);
  });

  it('should let the least recently used entry go past its limit', () => {
    const cache = new Lru<string, number>(2);
    cache.set('a', 1);
    cache.set('b', 2);
    cache.set('c', 3);

    expect(cache.get('a')).toBeUndefined();
  });

  it('should keep an entry read lately over an older one', () => {
    const cache = new Lru<string, number>(2);
    cache.set('a', 1);
    cache.set('b', 2);
    cache.get('a');
    cache.set('c', 3);

    expect([cache.get('a'), cache.get('b')]).toEqual([1, undefined]);
  });
});
