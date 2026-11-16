// Adding one contact detail, as the desktop's `ContactForm`: a small state machine from typing
// the address to confirming the link (email) or the code (phone).

import type { PendingContact } from '@better-matrix/matrix-native/types';

export type ContactStep =
  /** Typing the address or number. */
  | { type: 'entering' }
  /** The server is sending a link or code. */
  | { type: 'requesting' }
  /** The link or code is out; the user confirms it. `busy` while the code or the addition is on its way. */
  | { type: 'sent'; pending: PendingContact; busy: boolean };

export interface ContactForm {
  step: ContactStep;
  error: string | null;
}

export type ContactEvent =
  | { type: 'request' }
  | { type: 'sent'; pending: PendingContact }
  | { type: 'confirm' }
  /** The user gave up on the password: the detail still waits to be confirmed. */
  | { type: 'idle' }
  | { type: 'failed'; error: string }
  /** Added, or given up: back to an empty address. */
  | { type: 'reset' };

export const EMPTY_CONTACT_FORM: ContactForm = { step: { type: 'entering' }, error: null };

export function contactForm(form: ContactForm, event: ContactEvent): ContactForm {
  switch (event.type) {
    case 'request':
      return form.step.type === 'entering' ? { step: { type: 'requesting' }, error: null } : form;
    case 'sent':
      return form.step.type === 'requesting' ? { step: { type: 'sent', pending: event.pending, busy: false }, error: null } : form;
    case 'confirm':
      return form.step.type === 'sent' && !form.step.busy ? { step: { ...form.step, busy: true }, error: null } : form;
    case 'idle':
      return form.step.type === 'sent' ? { ...form, step: { ...form.step, busy: false } } : form;
    case 'failed':
      // A failed step goes back one: a sent detail can be confirmed again, a request typed again.
      return {
        step: form.step.type === 'sent' ? { ...form.step, busy: false } : { type: 'entering' },
        error: event.error,
      };
    case 'reset':
      return EMPTY_CONTACT_FORM;
  }
}

/** Whether a form can take a new request: entering, with something typed. */
export function canRequest(form: ContactForm, address: string): boolean {
  return form.step.type === 'entering' && address.trim() !== '';
}
