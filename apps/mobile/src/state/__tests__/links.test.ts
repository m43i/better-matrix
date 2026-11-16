import {
  findGifLink,
  findPreviewLink,
  gifSource,
  htmlWithoutLink,
  isLink,
  links,
  withoutLink,
} from '../links';

describe('links', () => {
  it('should be found without the punctuation around them', () => {
    expect(links('see (https://matrix.org/blog), then https://example.org.')).toEqual([
      'https://matrix.org/blog',
      'https://example.org',
    ]);
  });

  it('should be found by where they are after other letters', () => {
    expect(links('Grüße: https://a.example/ü und mehr')).toEqual(['https://a.example/ü']);
  });

  it('should not count a scheme alone', () => {
    expect(links('https:// http://')).toEqual([]);
  });

  it('should keep a closing parenthesis the link opened', () => {
    expect(findPreviewLink('see https://en.wikipedia.org/wiki/Rust_(programming_language).')).toBe(
      'https://en.wikipedia.org/wiki/Rust_(programming_language)',
    );
  });

  it('should count only a lone address as a link', () => {
    expect(isLink('  https://matrix.org/blog \n')).toBe(true);
    expect(isLink('see https://matrix.org')).toBe(false);
  });
});

describe('gifSource', () => {
  it('should point a Giphy page at its GIF', () => {
    expect(gifSource('https://giphy.com/gifs/cat-typing-JIX9t2j0ZTN9S')).toEqual({
      file: 'https://i.giphy.com/JIX9t2j0ZTN9S.gif',
    });
  });

  it('should read a Tenor share link from its page', () => {
    expect(gifSource('https://tenor.com/view/happy-cat-12345?x=1')).toEqual({
      page: 'https://tenor.com/view/happy-cat-12345',
    });
  });

  it('should point an Imgur page at its GIF', () => {
    expect(gifSource('https://imgur.com/AbC123')).toEqual({
      file: 'https://i.imgur.com/AbC123.gif',
    });
  });

  it('should make an Imgur gifv a GIF', () => {
    expect(gifSource('https://i.imgur.com/AbC123.gifv')).toEqual({
      file: 'https://i.imgur.com/AbC123.gif',
    });
  });

  it('should fetch a direct GIF link without its query', () => {
    expect(gifSource('https://media.tenor.com/abc/cat.gif?width=200')).toEqual({
      file: 'https://media.tenor.com/abc/cat.gif',
    });
  });

  it('should not count other links', () => {
    expect(gifSource('https://example.org/page')).toBeNull();
  });
});

describe('finding a message link', () => {
  it('should find the first GIF link', () => {
    expect(findGifLink('look (https://giphy.com/gifs/x-abc) and https://imgur.com/q')).toBe(
      'https://giphy.com/gifs/x-abc',
    );
  });

  it('should give a card to the first link that is not a GIF', () => {
    expect(
      findPreviewLink('https://giphy.com/gifs/cat-JIX9t2j0ZTN9S https://matrix.org/blog'),
    ).toBe('https://matrix.org/blog');
  });
});

describe('cutting a GIF link', () => {
  const url = 'https://media.tenor.com/x/kiwi.gif';

  it('should leave the words around it', () => {
    expect(withoutLink(`look ${url} at this`, url)).toBe('look at this');
  });

  it('should leave nothing of a message that is only the link', () => {
    expect(withoutLink(url, url)).toBe('');
  });

  it('should cut it from HTML with its anchor', () => {
    const link = 'https://giphy.com/gifs/ship-it?a=1&b=2';
    const html = '<b>Ship</b> <a href="https://giphy.com/gifs/ship-it?a=1&amp;b=2">link</a> now';
    expect(htmlWithoutLink(html, link)).toBe('<b>Ship</b>  now');
  });
});
