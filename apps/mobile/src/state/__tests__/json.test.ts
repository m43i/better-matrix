import { keepUnchanged, sameJson } from '../json';

describe('sameJson', () => {
  it('values with the same content should be the same', () => {
    expect(sameJson({ a: [1, { b: null }], c: 'x' }, { c: 'x', a: [1, { b: null }] })).toBe(true);
  });

  it('a different nested value should differ', () => {
    expect(sameJson({ a: [1, { b: null }] }, { a: [1, { b: false }] })).toBe(false);
  });

  it('an extra field should differ', () => {
    expect(sameJson({ a: 1 }, { a: 1, b: 2 })).toBe(false);
  });

  it('a list should differ from an object with the same entries', () => {
    expect(sameJson(['x'], { 0: 'x' })).toBe(false);
  });

  it('lists of other lengths should differ', () => {
    expect(sameJson([1, 2], [1])).toBe(false);
  });
});

describe('keepUnchanged', () => {
  const room = (room_id: string, unread = 0) => ({ room_id, unread });

  it('items with the same content should keep their earlier objects', () => {
    const before = [room('!a'), room('!b')];
    const after = keepUnchanged(before, [room('!a'), room('!b', 2)]);
    expect(after[0]).toBe(before[0]);
    expect(after[1]).toEqual(room('!b', 2));
  });

  it('a list with nothing changed should be the earlier list', () => {
    const before = [room('!a'), room('!b')];
    expect(keepUnchanged(before, [room('!a'), room('!b')])).toBe(before);
  });

  it('a removed item should change the list', () => {
    const before = [room('!a'), room('!b')];
    expect(keepUnchanged(before, [room('!a')])).toEqual([room('!a')]);
  });
});
