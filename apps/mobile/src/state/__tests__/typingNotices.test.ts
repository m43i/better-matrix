import { draftChanged, REPEAT_MS, typingEnded, typingFor } from '../typingNotices';

describe('typing notices', () => {
  it('should tell the room when the user starts typing', () => {
    expect(draftChanged(typingFor(''), 'h', 0).notice).toBe(true);
  });

  it('should not repeat the notice at once', () => {
    const typing = draftChanged(typingFor(''), 'h', 0).next;
    expect(draftChanged(typing, 'hi', 1000).notice).toBeNull();
  });

  it('should repeat the notice once the last is about to lapse', () => {
    const typing = draftChanged(typingFor(''), 'h', 0).next;
    expect(draftChanged(typing, 'hi', REPEAT_MS).notice).toBe(true);
  });

  it('should say typing stopped when the draft is emptied', () => {
    const typing = draftChanged(typingFor(''), 'h', 0).next;
    expect(draftChanged(typing, '', 10).notice).toBe(false);
  });

  it('should not count opening with a draft as typing', () => {
    expect(draftChanged(typingFor('old draft'), 'old draft', 0).notice).toBeNull();
  });

  it('should say typing stopped when the composer is left while typing', () => {
    const typing = draftChanged(typingFor(''), 'h', 0).next;
    expect(typingEnded(typing).notice).toBe(false);
  });

  it('should say nothing when the composer is left without typing', () => {
    expect(typingEnded(typingFor('draft')).notice).toBeNull();
  });
});
