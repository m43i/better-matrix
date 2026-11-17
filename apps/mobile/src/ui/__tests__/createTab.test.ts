import { createTab } from '../shell/create';

describe('createTab', () => {
  it('iOS 27 should give the create tab its own surface', () =>
    expect(createTab('ios', '27.0')).toBe('prominent'));

  it('iOS 26.1 should set the create tab apart as the search tab', () =>
    expect(createTab('ios', '26.1')).toBe('separate'));

  it('iOS 26.0 should show the create tab as one more tab', () =>
    expect(createTab('ios', '26.0')).toBe('tab'));

  it('older iOS should show the create tab as one more tab', () =>
    expect(createTab('ios', '18.5')).toBe('tab'));

  it('Android should have no create tab', () => expect(createTab('android', 35)).toBe('none'));
});
