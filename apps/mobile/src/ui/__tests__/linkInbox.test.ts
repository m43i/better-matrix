import { deliverLink, onLinkDelivered, takeLinks } from '../links/inbox';

describe('the link inbox', () => {
  beforeEach(() => void takeLinks());

  it('should keep links until they are taken, oldest first', () => {
    deliverLink('matrix:r/a:example.org');
    deliverLink('matrix:r/b:example.org');
    expect([takeLinks(), takeLinks()]).toEqual([
      ['matrix:r/a:example.org', 'matrix:r/b:example.org'],
      [],
    ]);
  });

  it('should tell the opener when a link comes', () => {
    const heard = jest.fn();
    const stop = onLinkDelivered(heard);
    deliverLink('matrix:u/grace:example.org');
    stop();
    deliverLink('matrix:u/kai:example.org');
    expect(heard).toHaveBeenCalledTimes(1);
  });
});
