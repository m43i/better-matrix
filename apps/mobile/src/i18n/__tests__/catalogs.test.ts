import { catalogs, mobileCatalogs, pluralForm, t, tp } from '../index';

describe('catalogs', () => {
  it.each(['de', 'fr'] as const)('should give %s every mobile key English has', (locale) => {
    const missing = Object.keys(mobileCatalogs.en).filter((key) => !(key in mobileCatalogs[locale]));
    expect(missing).toEqual([]);
  });

  it.each(['de', 'fr'] as const)('should give %s every placeholder English uses', (locale) => {
    const placeholders = (text: string) => [...text.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();
    const wrong = Object.entries(mobileCatalogs.en).filter(
      ([key, text]) => key in mobileCatalogs[locale] && placeholders(text).join() !== placeholders(mobileCatalogs[locale][key]!).join(),
    );
    expect(wrong.map(([key]) => key)).toEqual([]);
  });

  it('should fill placeholders', () => {
    expect(t('accounts.sign_out', { name: 'Ada' })).toBe('Sign out of Ada');
  });

  it('should pick the plural form for a count', () => {
    expect(tp('thread.replies', 1)).toBe(catalogs.en['thread.replies.one']!.replace('{{count}}', '1'));
  });
});

describe('pluralForm', () => {
  it('should use one for exactly one in English', () => expect(pluralForm('en', 1)).toBe('one'));
  it('should use other for none in German', () => expect(pluralForm('de', 0)).toBe('other'));
  it('should use one for none in French', () => expect(pluralForm('fr', 0)).toBe('one'));
  it('should use other for two in French', () => expect(pluralForm('fr', 2)).toBe('other'));
});
