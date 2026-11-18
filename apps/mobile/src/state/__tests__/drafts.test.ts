import { EMPTY_COMPOSER } from '../composer';
import { storedDraft, withKeptDraft } from '../drafts';
import { message } from '../../test/fixtures';

describe('kept drafts', () => {
  it('should keep nothing of a blank draft', () => {
    expect(storedDraft({ ...EMPTY_COMPOSER, draft: '  \n' })).toBeNull();
  });

  it('should keep a draft’s formatting as the desktop sends it', () => {
    expect(storedDraft({ ...EMPTY_COMPOSER, draft: '**hi** there' })).toEqual({
      body: 'hi there',
      html: '<p><strong>hi</strong> there</p>',
    });
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

  it('should leave what was typed meanwhile', () => {
    const typed = { ...EMPTY_COMPOSER, draft: 'new words' };

    expect(withKeptDraft(typed, { body: 'old', html: null })).toBe(typed);
  });

  it('should leave a composer answering a message', () => {
    const replying = { ...EMPTY_COMPOSER, replyTo: message('$q') };

    expect(withKeptDraft(replying, { body: 'old', html: null })).toBe(replying);
  });
});
