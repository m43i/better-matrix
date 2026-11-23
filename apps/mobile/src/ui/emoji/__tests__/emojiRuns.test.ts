import { emojiRuns } from '../emojiRuns';

it('should leave digits, text symbols and explicit text presentation in the surrounding font', () => {
  const text = '1 # * © ♥ ☕\uFE0E';

  expect(emojiRuns(text)).toEqual([text]);
});

it.each(['👩🏽‍💻', '🇩🇪', '1️⃣', '❤️', '🙂'])('should keep %s in a single emoji run', (emoji) => {
  expect(emojiRuns(`before ${emoji} after`)).toEqual(['before ', emoji, ' after']);
});
