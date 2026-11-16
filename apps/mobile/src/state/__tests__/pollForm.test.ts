import {
  addAnswer,
  MAX_ANSWERS,
  newPollForm,
  pollOutgoing,
  pollReady,
  removeAnswer,
  setAnswer,
  type PollForm,
} from '../pollForm';

function form(question: string, ...answers: string[]): PollForm {
  return { ...newPollForm(), question, answers };
}

describe('the poll form', () => {
  it('should start with two empty answers, showing results as people vote', () => {
    expect(newPollForm()).toEqual({ question: '', answers: ['', ''], disclosed: true });
  });

  it('should need a question', () => {
    expect(pollReady(form(' ', 'a', 'b'))).toBe(false);
  });

  it('should need two written answers', () => {
    expect(pollReady(form('Lunch?', 'Pizza', '  '))).toBe(false);
  });

  it('should send the trimmed question and written answers', () => {
    expect(pollOutgoing({ question: ' Lunch? ', answers: ['Pizza ', '', 'Soup'], disclosed: false })).toEqual({
      type: 'Poll',
      data: { question: 'Lunch?', answers: ['Pizza', 'Soup'], disclosed: false },
    });
  });

  it('should not send a poll that is not ready', () => {
    expect(pollOutgoing(form('Lunch?', 'Pizza'))).toBeNull();
  });

  it('should stop adding answers at twenty', () => {
    const full = form('q', ...Array.from({ length: MAX_ANSWERS }, (_, i) => String(i)));
    expect(addAnswer(full).answers).toHaveLength(MAX_ANSWERS);
  });

  it('should keep two answers when one is removed', () => {
    expect(removeAnswer(form('q', 'a', 'b'), 0).answers).toEqual(['a', 'b']);
  });

  it('should remove the answer asked for', () => {
    expect(removeAnswer(form('q', 'a', 'b', 'c'), 1).answers).toEqual(['a', 'c']);
  });

  it('should change one answer', () => {
    expect(setAnswer(form('q', 'a', 'b'), 1, 'B').answers).toEqual(['a', 'B']);
  });
});
