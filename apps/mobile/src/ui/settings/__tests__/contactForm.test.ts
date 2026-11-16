import type { PendingContact } from '@better-matrix/matrix-native/types';

import { canRequest, type ContactForm, contactForm, EMPTY_CONTACT_FORM } from '../contactForm';

const pending: PendingContact = { kind: 'Phone', address: '491701234567', secret: 's', sid: '1', submit_url: null };
const requesting = contactForm(EMPTY_CONTACT_FORM, { type: 'request' });
const sent = contactForm(requesting, { type: 'sent', pending });

describe('contactForm', () => {
  it('should wait for the code once requested', () => {
    expect(requesting.step).toEqual({ type: 'requesting' });
  });

  it('should hold the detail to confirm once the code is sent', () => {
    expect(sent.step).toEqual({ type: 'sent', pending, busy: false });
  });

  it('should be busy while the code is being confirmed', () => {
    expect(contactForm(sent, { type: 'confirm' }).step).toEqual({ type: 'sent', pending, busy: true });
  });

  it('should keep the detail to confirm when confirming fails', () => {
    const failed = contactForm(contactForm(sent, { type: 'confirm' }), { type: 'failed', error: 'wrong code' });
    expect(failed).toEqual<ContactForm>({ step: { type: 'sent', pending, busy: false }, error: 'wrong code' });
  });

  it('should go back to typing when the request fails', () => {
    expect(contactForm(requesting, { type: 'failed', error: 'invalid' })).toEqual({ step: { type: 'entering' }, error: 'invalid' });
  });

  it('should still wait for confirmation when the password is given up', () => {
    expect(contactForm(contactForm(sent, { type: 'confirm' }), { type: 'idle' }).step).toEqual({ type: 'sent', pending, busy: false });
  });

  it('should start over once the detail is added', () => {
    expect(contactForm(sent, { type: 'reset' })).toBe(EMPTY_CONTACT_FORM);
  });

  it('should not request a code while one is on its way', () => {
    expect(contactForm(requesting, { type: 'request' })).toBe(requesting);
  });

  it('should not request a code for a blank address', () => {
    expect(canRequest(EMPTY_CONTACT_FORM, '  ')).toBe(false);
  });
});
