import { storeKey } from '../secrets';

describe('storeKey', () => {
  it('should keep letters, digits, dots and dashes', () => expect(storeKey('a.B-9')).toBe('a.B-9'));
  it('should escape what the keychain refuses', () =>
    expect(storeKey('s/@a:x')).toBe('s_2f_40a_3ax'));
  it('should escape underscores so two keys never meet', () =>
    expect(storeKey('_40')).not.toBe(storeKey('@')));
});
