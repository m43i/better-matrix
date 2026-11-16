import { detachedEngine } from '@better-matrix/matrix-native/client';

import { ADA, GRACE } from '../../../test/fixtures';
import { Store } from '../../store';
import { accountPushOn, pushOffWith, setAccountPush, setPushEnabled } from '../push';

function store() {
  return new Store(detachedEngine().engine);
}

describe('push settings', () => {
  it('turning an account off should keep the others on', () => {
    const s = store();
    setAccountPush(s, GRACE, false);
    expect([accountPushOn(s.getState().settings, ADA), accountPushOn(s.getState().settings, GRACE)]).toEqual([true, false]);
  });

  it('turning an account on again should forget it was off', () => {
    expect(pushOffWith([ADA, GRACE], GRACE, true)).toEqual([ADA]);
  });

  it('turning an account off twice should list it once', () => {
    expect(pushOffWith([GRACE], GRACE, false)).toEqual([GRACE]);
  });

  it('push off on the device should turn every account off', () => {
    const s = store();
    setPushEnabled(s, false);
    expect(accountPushOn(s.getState().settings, ADA)).toBe(false);
  });
});
