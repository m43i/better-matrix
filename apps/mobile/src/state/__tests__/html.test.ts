import { type Block, blocksText, htmlBlocks, plainBlocks } from '../html';

const paragraph = (blocks: Block[]) => {
  const first = blocks[0];

  if (first?.type !== 'paragraph') throw new Error(`not a paragraph: ${JSON.stringify(first)}`);

  return first.inlines;
};

describe('htmlBlocks', () => {
  it('should read bold and italic as marks', () => {
    expect(paragraph(htmlBlocks('<b>a</b><em>b</em>'))).toEqual([
      { type: 'text', text: 'a', marks: { bold: true } },
      { type: 'text', text: 'b', marks: { italic: true } },
    ]);
  });

  it('should nest marks', () => {
    expect(paragraph(htmlBlocks('<strong><del>gone</del></strong>'))).toEqual([
      { type: 'text', text: 'gone', marks: { bold: true, strike: true } },
    ]);
  });

  it('should keep a web link', () => {
    expect(paragraph(htmlBlocks('<a href="https://matrix.org">Matrix</a>'))).toEqual([
      { type: 'text', text: 'Matrix', marks: { link: 'https://matrix.org' } },
    ]);
  });

  it('should drop a link that runs script', () => {
    expect(paragraph(htmlBlocks('<a href="javascript:alert(1)">x</a>'))).toEqual([
      { type: 'text', text: 'x', marks: {} },
    ]);
  });

  it('should show a script as nothing', () => {
    expect(htmlBlocks('<script>alert(1)</script>')).toEqual([]);
  });

  it('should show unknown tags as their text', () => {
    expect(paragraph(htmlBlocks('<marquee>hi</marquee>'))).toEqual([
      { type: 'text', text: 'hi', marks: {} },
    ]);
  });

  it('should read a stray angle bracket as text', () => {
    expect(paragraph(htmlBlocks('1 < 2'))).toEqual([{ type: 'text', text: '1 < 2', marks: {} }]);
  });

  it('should decode entities', () => {
    expect(paragraph(htmlBlocks('a &amp; b &#x1F600; &lt;3'))).toEqual([
      { type: 'text', text: 'a & b 😀 <3', marks: {} },
    ]);
  });

  it('should leave out the reply fallback', () => {
    expect(paragraph(htmlBlocks('<mx-reply><blockquote>old</blockquote></mx-reply>new'))).toEqual([
      { type: 'text', text: 'new', marks: {} },
    ]);
  });

  it('should make a matrix.to link to a person a mention', () => {
    expect(
      paragraph(htmlBlocks('hi <a href="https://matrix.to/#/%40ada%3Aexample.org">Ada</a>')),
    ).toEqual([
      { type: 'text', text: 'hi ', marks: {} },
      { type: 'mention', kind: 'user', id: '@ada:example.org', label: 'Ada' },
    ]);
  });

  it('should make a matrix.to link to a room a mention', () => {
    expect(
      paragraph(htmlBlocks('<a href="https://matrix.to/#/#general:example.org">#general</a>')),
    ).toEqual([{ type: 'mention', kind: 'room', id: '#general:example.org', label: '#general' }]);
  });

  it('should keep a matrix.to link to a message a link, so it opens at the message', () => {
    const href = 'https://matrix.to/#/!lab:example.org/$event?via=example.org';
    expect(paragraph(htmlBlocks(`<a href="${href}">that</a>`))).toEqual([
      { type: 'text', text: 'that', marks: { link: href } },
    ]);
  });

  it('should make a matrix: URI to a person a mention', () => {
    expect(paragraph(htmlBlocks('<a href="matrix:u/ada:example.org">Ada</a>'))).toEqual([
      { type: 'mention', kind: 'user', id: '@ada:example.org', label: 'Ada' },
    ]);
  });

  it('should collapse spaces from the markup', () => {
    expect(paragraph(htmlBlocks('<p>\n  one\n  two  </p>'))).toEqual([
      { type: 'text', text: 'one two', marks: {} },
    ]);
  });

  it('should keep line breaks', () => {
    expect(paragraph(htmlBlocks('one<br>two'))).toEqual([
      { type: 'text', text: 'one\ntwo', marks: {} },
    ]);
  });

  it('should keep a code block as written', () => {
    expect(
      htmlBlocks('<pre><code class="language-rust">fn main() {\n    run();\n}\n</code></pre>'),
    ).toEqual([{ type: 'code', text: 'fn main() {\n    run();\n}' }]);
  });

  it('should read paragraphs as blocks', () => {
    expect(htmlBlocks('<p>one</p><p>two</p>').map((b) => b.type)).toEqual([
      'paragraph',
      'paragraph',
    ]);
  });

  it('should read a list with its items', () => {
    expect(htmlBlocks('<ol start="3"><li>a</li><li>b</li></ol>')).toEqual([
      {
        type: 'list',
        ordered: true,
        start: 3,
        items: [
          [{ type: 'paragraph', inlines: [{ type: 'text', text: 'a', marks: {} }] }],
          [{ type: 'paragraph', inlines: [{ type: 'text', text: 'b', marks: {} }] }],
        ],
      },
    ]);
  });

  it('should read a quote with its blocks', () => {
    expect(htmlBlocks('<blockquote><p>said</p></blockquote>')).toEqual([
      {
        type: 'quote',
        blocks: [{ type: 'paragraph', inlines: [{ type: 'text', text: 'said', marks: {} }] }],
      },
    ]);
  });

  it('should read a heading with its level', () => {
    expect(htmlBlocks('<h2>Title</h2>')).toEqual([
      { type: 'heading', level: 2, inlines: [{ type: 'text', text: 'Title', marks: {} }] },
    ]);
  });

  it('should make bare addresses links', () => {
    expect(paragraph(htmlBlocks('see https://matrix.org now'))).toEqual([
      { type: 'text', text: 'see ', marks: {} },
      { type: 'text', text: 'https://matrix.org', marks: { link: 'https://matrix.org' } },
      { type: 'text', text: ' now', marks: {} },
    ]);
  });

  it('should not make addresses in code links', () => {
    expect(paragraph(htmlBlocks('<code>https://matrix.org</code>'))).toEqual([
      { type: 'text', text: 'https://matrix.org', marks: { code: true } },
    ]);
  });

  it('should survive deep nesting', () => {
    const html = '<b>'.repeat(5000) + 'deep' + '</b>'.repeat(5000);
    expect(blocksText(htmlBlocks(html))).toBe('deep');
  });
});

describe('plainBlocks', () => {
  it('should keep the text as written', () => {
    expect(paragraph(plainBlocks('one\n  two'))).toEqual([
      { type: 'text', text: 'one\n  two', marks: {} },
    ]);
  });

  it('should make addresses links', () => {
    expect(paragraph(plainBlocks('https://matrix.org'))).toEqual([
      { type: 'text', text: 'https://matrix.org', marks: { link: 'https://matrix.org' } },
    ]);
  });
});

describe('blocksText', () => {
  it('should read mentions by their label', () => {
    expect(
      blocksText(htmlBlocks('hi <a href="https://matrix.to/#/@ada:example.org">Ada</a>')),
    ).toBe('hi Ada');
  });
});

describe('htmlBlocks with Matrix’s inline objects', () => {
  it('should hide a spoiler’s words behind a spoiler', () => {
    expect(paragraph(htmlBlocks('he <span data-mx-spoiler="plot">dies</span>'))).toEqual([
      { type: 'text', text: 'he ', marks: {} },
      { type: 'spoiler', reason: 'plot', text: 'dies' },
    ]);
  });

  it('should read a formula as Unicode, keeping its LaTeX', () => {
    expect(paragraph(htmlBlocks('<span data-mx-maths="x^2"><code>x^2</code></span>'))).toEqual([
      { type: 'math', latex: 'x^2', text: 'x²', block: false },
    ]);
  });

  it('should give a block formula a line of its own', () => {
    expect(
      htmlBlocks('a<div data-mx-maths="\\frac{1}{2}"><pre><code>\\frac{1}{2}</code></pre></div>b'),
    ).toEqual([
      { type: 'paragraph', inlines: [{ type: 'text', text: 'a', marks: {} }] },
      {
        type: 'paragraph',
        inlines: [{ type: 'math', latex: '\\frac{1}{2}', text: '1/2', block: true }],
      },
      { type: 'paragraph', inlines: [{ type: 'text', text: 'b', marks: {} }] },
    ]);
  });

  it('should show a custom emoji by its image', () => {
    expect(
      paragraph(htmlBlocks('<img data-mx-emoticon src="mxc://x.org/cat" alt=":cat:" height="32">')),
    ).toEqual([{ type: 'emote', url: 'mxc://x.org/cat', label: ':cat:' }]);
  });

  it('should show a custom emoji the sanitiser left unmarked by its image', () => {
    expect(
      paragraph(htmlBlocks('<img src="mxc://x.org/cat" alt=":blobcat:" height="32">')),
    ).toEqual([{ type: 'emote', url: 'mxc://x.org/cat', label: ':blobcat:' }]);
  });

  it('should label a custom emoji without a description by its title', () => {
    expect(paragraph(htmlBlocks('<img src="mxc://x.org/cat" title=":blobcat:">'))).toEqual([
      { type: 'emote', url: 'mxc://x.org/cat', label: ':blobcat:' },
    ]);
  });

  it('should show an image from the web by its description', () => {
    expect(paragraph(htmlBlocks('<img src="https://x.org/a.png" alt="a cat">'))).toEqual([
      { type: 'text', text: 'a cat', marks: {} },
    ]);
  });

  it('should read a spoiler’s words as text for copying', () => {
    expect(
      blocksText(
        htmlBlocks('a <span data-mx-spoiler>b</span> <span data-mx-maths="\\pi">π</span>'),
      ),
    ).toBe('a b π');
  });

  it('should read a spoiler as plain words, its links, mentions and pictures as text', () => {
    expect(
      paragraph(
        htmlBlocks(
          '<span data-mx-spoiler><a href="https://matrix.to/#/@grace:x.org">Grace</a> at <a href="https://x.org">x.org</a> <img src="mxc://x.org/c" alt=":cat:"></span>',
        ),
      ),
    ).toEqual([{ type: 'spoiler', reason: '', text: 'Grace at x.org :cat:' }]);
  });

  it('should keep a spoiler’s formatting and line breaks out of it but its words', () => {
    expect(
      paragraph(htmlBlocks('<span data-mx-spoiler>he <b>dies</b><br />  sadly</span>')),
    ).toEqual([{ type: 'spoiler', reason: '', text: 'he dies\nsadly' }]);
  });

  it('should not make a mention of a link around a spoiler, keeping the spoiler hidden', () => {
    expect(
      paragraph(
        htmlBlocks(
          '<a href="https://matrix.to/#/@x:y.org"><span data-mx-spoiler>secret</span></a>',
        ),
      ),
    ).toEqual([{ type: 'spoiler', reason: '', text: 'secret' }]);
  });

  it('should not make a link of a web link around a spoiler', () => {
    expect(
      paragraph(
        htmlBlocks('<a href="https://x.org/secret">see <span data-mx-spoiler>it</span></a>'),
      ),
    ).toEqual([
      { type: 'text', text: 'see ', marks: {} },
      { type: 'spoiler', reason: '', text: 'it' },
    ]);
  });

  it('should not show a mention or custom emoji inside a spoiler, nor link a web address there', () => {
    const inlines = paragraph(
      htmlBlocks(
        '<span data-mx-spoiler><a href="https://matrix.to/#/@x:y.org">X</a> <img src="mxc://y.org/e" alt=":e:"> https://y.org</span>',
      ),
    );

    expect(inlines.map((inline) => inline.type)).toEqual(['spoiler']);
  });

  it('should read a span marked both spoiler and formula as a spoiler', () => {
    expect(
      paragraph(htmlBlocks('<span data-mx-spoiler data-mx-maths="x^2"><code>x^2</code></span>')),
    ).toEqual([{ type: 'spoiler', reason: '', text: 'x^2' }]);
  });

  it('should read a block marked both spoiler and formula as a spoiler', () => {
    expect(paragraph(htmlBlocks('<div data-mx-maths="x^2" data-mx-spoiler>x^2</div>'))).toEqual([
      { type: 'spoiler', reason: '', text: 'x^2' },
    ]);
  });

  it('should read a code block holding a spoiler as its lines of text, the spoiler hidden', () => {
    expect(htmlBlocks('<pre><code>a <span data-mx-spoiler>b</span>\nc\n</code></pre>')).toEqual([
      {
        type: 'paragraph',
        inlines: [
          { type: 'text', text: 'a ', marks: {} },
          { type: 'spoiler', reason: '', text: 'b' },
          { type: 'text', text: '\nc', marks: {} },
        ],
      },
    ]);
  });

  it('should keep a code block without a spoiler code', () => {
    expect(htmlBlocks('<pre><code>a <b>b</b>\nc</code></pre>')).toEqual([
      { type: 'code', text: 'a b\nc' },
    ]);
  });
});
