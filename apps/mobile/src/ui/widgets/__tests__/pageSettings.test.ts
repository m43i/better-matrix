import { PAGE_SETTINGS } from '../pageSettings';

describe('the widget web view', () => {
  it('should give no widget the user’s location', () => {
    expect(PAGE_SETTINGS.geolocationEnabled).toBe(false);
  });

  it('should let every scheme reach the navigation policy', () => {
    expect(PAGE_SETTINGS.originWhitelist).toEqual(['*']);
  });

  it('should inject its scripts into the main frame only', () => {
    expect(PAGE_SETTINGS.injectedJavaScriptBeforeContentLoadedForMainFrameOnly).toBe(true);
  });
});
