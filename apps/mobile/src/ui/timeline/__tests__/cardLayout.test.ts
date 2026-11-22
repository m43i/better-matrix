import { cardLayout } from '../cardLayout';

describe('a link card’s picture', () => {
  it('should sit beside the words in a wide card', () => {
    expect(cardLayout(360)).toBe('beside');
  });

  it('should go under the words in a narrow card (a bubble), rather than be left out', () => {
    expect(cardLayout(180)).toBe('below');
  });
});
