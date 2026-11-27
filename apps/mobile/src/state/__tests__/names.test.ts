import { roomSummary } from '../../test/fixtures';
import { personName, roomName } from '../names';

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
