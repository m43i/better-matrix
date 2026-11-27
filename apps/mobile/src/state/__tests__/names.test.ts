import { roomSummary } from '../../test/fixtures';
import { mentionName, personName, roomName } from '../names';

describe('personName', () => {
  it('should preserve the original name when the preference is off', () => {
    expect(personName('Hopper, Grace', false)).toBe('Hopper, Grace');
  });

  it('should reverse both parts of a matching name and keep the comma', () => {
    expect(personName('  García Márquez, Gabriel José  ', true)).toBe(
      'Gabriel José, García Márquez',
    );
  });

  it.each([
    'Grace Hopper',
    'Hopper,',
    ', Grace',
    'Hopper, Grace, Jr.',
    '@hopper,grace:example.org',
  ])('should keep %s as entered', (name) => expect(personName(name, true)).toBe(name));
});

describe('roomName', () => {
  it('should show the reordered name of a direct chat partner without changing the room summary', () => {
    const room = { ...roomSummary('!dm:x', 'Hopper, Grace', 'Direct'), name_from_member: true };
    expect(roomName(room, true)).toBe('Grace, Hopper');
    expect(room.name).toBe('Hopper, Grace');
  });

  it('should leave an explicitly named direct chat alone', () => {
    const room = roomSummary('!dm:x', 'Design, Support', 'Direct');
    expect(roomName(room, true)).toBe('Design, Support');
  });
});

describe('mentionName', () => {
  it('should keep the mention prefix with the displayed name', () => {
    expect(mentionName('@Hopper, Grace', '@grace:x', true)).toBe('@Grace, Hopper');
  });

  it('should keep the user ID when there is no display label', () => {
    const id = '@hopper,grace:example.org';
    expect(mentionName(id, id, true)).toBe(id);
  });

  it('should reverse a mention label whose display name contains a colon', () => {
    expect(mentionName('@Hopper: Ops, Grace', '@grace:x', true)).toBe('@Grace, Hopper: Ops');
  });
});
