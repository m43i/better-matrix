import { EMPTY_COMPOSER } from '../composer';
import { storedDraft, withKeptDraft } from '../drafts';
import { message } from '../../test/fixtures';

describe('kept drafts', () => {
  it('should keep nothing of a blank draft', () => {
    expect(storedDraft({ ...EMPTY_COMPOSER, draft: '  \n' })).toBeNull();
  });

  it('should keep a draft exactly as typed, markers, escapes and spaces included', () => {
    const typed = '**hi** \\*literal\\* ';
    const kept = storedDraft({ ...EMPTY_COMPOSER, draft: typed });

    expect(kept && withKeptDraft(undefined, kept).draft).toBe(typed);
  });

  it('should keep the draft set aside under an edit, not the edit', () => {
    const editing = {
      ...EMPTY_COMPOSER,
      editing: message('$mine'),
      draft: 'the edit',
      setAside: { draft: 'the draft', replyTo: null, attachments: [], original: 'old' },
    };

    expect(storedDraft(editing)).toEqual({ body: 'the draft', html: null });
  });

  it('should bring a kept draft back into an empty composer', () => {
    expect(withKeptDraft(undefined, { body: 'hi there', html: '<em>hi</em> there' }).draft).toBe(
      '_hi_ there',
    );
  });

  it('should leave a composer the user typed in and cleared again', () => {
    const cleared = { ...EMPTY_COMPOSER, touched: true };

    expect(withKeptDraft(cleared, { body: 'old', html: null })).toBe(cleared);
  });

  it('should leave what was typed meanwhile', () => {
    const typed = { ...EMPTY_COMPOSER, draft: 'new words' };

    expect(withKeptDraft(typed, { body: 'old', html: null })).toBe(typed);
  });

  it('should bring a kept draft back beside a reply picked meanwhile', () => {
    const replying = { ...EMPTY_COMPOSER, replyTo: message('$q') };

    expect(withKeptDraft(replying, { body: 'old', html: null })).toMatchObject({
      draft: 'old',
      replyTo: message('$q'),
    });
  });
});
