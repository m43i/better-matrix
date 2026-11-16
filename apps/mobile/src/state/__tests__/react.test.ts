import { selectFrom, shallow } from '../react';

interface S {
  rooms: Record<string, string>;
}

const state: S = { rooms: { a: 'Alpha', b: 'Beta' } };

describe('selectFrom', () => {
  it('a new selector should read again from the same state', () => {
    const first = selectFrom(null, state, (s: S) => s.rooms.a, Object.is);
    const second = selectFrom(first, state, (s: S) => s.rooms.b, Object.is);
    expect(second.value).toBe('Beta');
  });

  it('the same selector on the same state should not read again', () => {
    const selector = jest.fn((s: S) => s.rooms.a);
    const first = selectFrom(null, state, selector, Object.is);
    selectFrom(first, state, selector, Object.is);
    expect(selector).toHaveBeenCalledTimes(1);
  });

  it('an equal value from a new state should keep its reference', () => {
    const selector = (s: S) => Object.values(s.rooms);
    const first = selectFrom(null, state, selector, shallow);
    const second = selectFrom(first, { rooms: { ...state.rooms } }, selector, shallow);
    expect(second.value).toBe(first.value);
  });
});

describe('shallow', () => {
  it('different undefined-valued properties should not compare equal', () => {
    expect(shallow({ first: undefined }, { second: undefined })).toBe(false);
  });

  it('objects with the same properties in a different order should compare equal', () => {
    expect(shallow({ first: 1, second: 2 }, { second: 2, first: 1 })).toBe(true);
  });
});
