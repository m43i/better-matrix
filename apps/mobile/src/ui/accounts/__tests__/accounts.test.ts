import { OAUTH_REDIRECT } from '../../../engine/identity';
import { unverifiedAccount } from '../../../state/actions/security';
import { apply, initialState } from '../../../state/reducer';
import { ADA, summary } from '../../../test/fixtures';
import { isOAuthRedirect } from '../redirect';

describe('isOAuthRedirect', () => {
  it('should recognise the redirect with its answer', () => {
    expect(isOAuthRedirect(`${OAUTH_REDIRECT}?code=1&state=2`)).toBe(true);
  });

  it('should recognise the path the router makes of it', () => {
    expect(isOAuthRedirect('/oauth?code=1')).toBe(true);
  });

  it('should leave other links to the router', () => {
    expect(isOAuthRedirect('/oauthorised')).toBe(false);
  });
});

describe('unverifiedAccount', () => {
  const signedIn = apply(initialState(), { type: 'AccountAdded', data: summary(ADA) });

  it('should name the account on screen when its session is unverified', () => {
    const state = apply(signedIn, {
      type: 'SecurityChanged',
      data: {
        account: ADA,
        security: { session_verified: false, recovery: 'Enabled', offline_device: 'Off' },
      },
    });

    expect(unverifiedAccount(state)).toBe(ADA);
  });

  it('should name no account before its security is known', () => {
    expect(unverifiedAccount(signedIn)).toBeNull();
  });
});
