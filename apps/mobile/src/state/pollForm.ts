// A poll being written (the desktop's `state/poll.rs` `PollForm`): a question, two to twenty
// answers, and whether votes show while it's open. Blank answers are left out on send; the
// engine refuses fewer than two (`Error::InvalidPoll`), so the form never offers that.

import type { Outgoing } from '@better-matrix/matrix-native/types';

/** The most answers a poll may have (MSC3381). */
export const MAX_ANSWERS = 20;
/** The fewest answer fields the form shows. */
export const MIN_ANSWERS = 2;

export interface PollForm {
  question: string;
  answers: string[];
  disclosed: boolean;
}

/** A blank poll with two empty answers; results show as people vote, as on the desktop. */
export function newPollForm(): PollForm {
  return { question: '', answers: ['', ''], disclosed: true };
}

/** The answers written so far, trimmed, blanks left out. */
export function writtenAnswers(form: PollForm): string[] {
  return form.answers.map((answer) => answer.trim()).filter((answer) => answer !== '');
}

/** Whether it can be sent: a question and two to twenty answers. */
export function pollReady(form: PollForm): boolean {
  const written = writtenAnswers(form).length;
  return form.question.trim() !== '' && written >= MIN_ANSWERS && written <= MAX_ANSWERS;
}

/** The poll to send, once ready. */
export function pollOutgoing(form: PollForm): Outgoing | null {
  if (!pollReady(form)) return null;
  return { type: 'Poll', data: { question: form.question.trim(), answers: writtenAnswers(form), disclosed: form.disclosed } };
}

/** Whether another answer may be added. */
export function canAddAnswer(form: PollForm): boolean {
  return form.answers.length < MAX_ANSWERS;
}

/** An empty answer added at the end, up to the limit. */
export function addAnswer(form: PollForm): PollForm {
  return canAddAnswer(form) ? { ...form, answers: [...form.answers, ''] } : form;
}

/** Whether answers may be removed: two always stay. */
export function canRemoveAnswer(form: PollForm): boolean {
  return form.answers.length > MIN_ANSWERS;
}

/** The answer at `index` removed, keeping two at least. */
export function removeAnswer(form: PollForm, index: number): PollForm {
  if (!canRemoveAnswer(form) || index < 0 || index >= form.answers.length) return form;
  return { ...form, answers: form.answers.filter((_, i) => i !== index) };
}

/** The answer at `index` changed to `text`. */
export function setAnswer(form: PollForm, index: number, text: string): PollForm {
  if (index < 0 || index >= form.answers.length) return form;
  return { ...form, answers: form.answers.map((answer, i) => (i === index ? text : answer)) };
}
