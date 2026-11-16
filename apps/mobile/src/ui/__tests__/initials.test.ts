import { initials } from '../initials';

describe('initials', () => {
  it('should use one letter for a single word', () => expect(initials('ada')).toBe('A'));
  it('should take the first letters of the first two words', () =>
    expect(initials('Grace Brewster Hopper')).toBe('GB'));
  it('should use the local part of a user id', () =>
    expect(initials('@ada:example.org')).toBe('A'));
  it('should skip symbols at the start of a word', () => expect(initials('lab (old)')).toBe('LO'));
  it('should fall back to a question mark when blank', () => expect(initials('  ')).toBe('?'));
});
