import vectors from '../../../../../packages/matrix-core/src/permalinks.json';
import typedVectors from '../../../../../packages/matrix-core/src/typed_links.json';
import { isPermalink, parsePermalink, parseTyped, permalinkUri } from '../permalinks';

describe('parsePermalink', () => {
  // The engine's parser is tested against the same vectors (packages/matrix-core/src/permalinks.rs).
  it.each(vectors.map((v) => [v.link, v.parsed] as const))(
    'should read %j as the engine does',
    (link, parsed) => {
      expect(parsePermalink(link)).toEqual(parsed);
    },
  );

  it('should be tested against more than a handful of vectors', () =>
    expect(vectors.length).toBeGreaterThan(30));
});

describe('isPermalink', () => {
  it('should not take a web page for a permalink', () =>
    expect(isPermalink('https://example.org/#/@alice:example.org')).toBe(false));
  it('should take a matrix.to link for one', () =>
    expect(isPermalink('https://matrix.to/#/#rust:matrix.org')).toBe(true));
});

describe('parseTyped', () => {
  // The engine reads typed links against the same vectors (`Permalink::parse_typed`).
  it.each(typedVectors.map((v) => [v.text, v.parsed] as const))(
    'should read %j as the engine does',
    (text, parsed) => {
      expect(parseTyped(text)).toEqual(parsed);
    },
  );
});

describe('permalinkUri', () => {
  const links = vectors.flatMap((v) => {
    const link = parsePermalink(v.link);

    return link === null ? [] : [link];
  });

  it.each(links.map((link) => [permalinkUri(link), link] as const))(
    'should read %j back as the link it was written from',
    (uri, link) => {
      expect(parsePermalink(uri)).toEqual(link);
    },
  );
});
