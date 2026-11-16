import { redirectSystemPath } from '../../app/+native-intent';
import { takeLinks } from '../links/inbox';

describe('links into the app', () => {
  beforeEach(() => void takeLinks());

  it('should keep a matrix: link for the opener and start at Home', () => {
    expect(redirectSystemPath({ path: 'matrix:r/rust:matrix.org', initial: true })).toBe('/');
    expect(takeLinks()).toEqual(['matrix:r/rust:matrix.org']);
  });

  it('should keep a matrix.to link that comes while the app runs, leaving the router where it is', () => {
    expect(
      redirectSystemPath({ path: 'https://matrix.to/#/@grace:example.org', initial: false }),
    ).toBeNull();
    expect(takeLinks()).toEqual(['https://matrix.to/#/@grace:example.org']);
  });

  it('should route other links as they are', () => {
    expect(redirectSystemPath({ path: 'bettermatrix://settings', initial: false })).toBe(
      'bettermatrix://settings',
    );
    expect(takeLinks()).toEqual([]);
  });
});
