import { routes, widgetTarget } from '../routes';
import type { WidgetTarget } from '../state/widgets';

/** Each thing the widget screen runs, with the params its route carries for it. */
const RUNS: Array<[WidgetTarget, Record<string, string>]> = [
  [{ type: 'widget', widgetId: 'w1' }, { widgetId: 'w1' }],
  [{ type: 'meeting', video: true }, { meeting: 'video' }],
  [{ type: 'meeting', video: false }, { meeting: 'voice' }],
  [{ type: 'integrations' }, { integrations: '1' }],
];

describe('the widget screen’s route', () => {
  it.each(RUNS)('should carry what it runs and the app’s launch: %j', (target, params) => {
    expect(routes.widget('@ada:x', '!r:x', target, 'nonce-1')).toEqual({
      pathname: '/widget',
      params: { account: '@ada:x', roomId: '!r:x', ...params, launch: 'nonce-1' },
    });
  });

  it.each(RUNS)('should read back what it runs: %j', (target, params) => {
    expect(widgetTarget(params)).toEqual(target);
  });

  it('should run nothing without params naming it', () => {
    expect(widgetTarget({})).toBeNull();
  });
});
