import { activeBlocks, applyFormat, applyLink, continueBlock, draftOf, outgoingText } from '../format';

const caret = (at: number) => ({ start: at, end: at });

describe('outgoingText', () => {
  it('should send plain text without html', () => {
    expect(outgoingText('hello')).toEqual({ body: 'hello', html: null });
  });

  it('should trim the body', () => {
    expect(outgoingText('  hello \n')).toEqual({ body: 'hello', html: null });
  });

  it('should send bold text as strong, as the desktop does', () => {
    expect(outgoingText('one **two**')).toEqual({ body: 'one two', html: '<p>one <strong>two</strong></p>' });
  });

  it('should send italic, strike and code', () => {
    expect(outgoingText('_a_ ~~b~~ `c`').html).toBe('<p><em>a</em> <s>b</s> <code>c</code></p>');
  });

  it('should take a single star as italic', () => {
    expect(outgoingText('*a*').html).toBe('<p><em>a</em></p>');
  });

  it('should nest marks in the canonical order, one window at a time', () => {
    expect(outgoingText('**bold _both_**').html).toBe('<p><strong>bold </strong><strong><em>both</em></strong></p>');
  });

  it('should send a link with its address', () => {
    expect(outgoingText('see [the blog](https://matrix.org/blog)')).toEqual({
      body: 'see the blog',
      html: '<p>see <a href="https://matrix.org/blog">the blog</a></p>',
    });
  });

  it('should keep snake_case words plain', () => {
    expect(outgoingText('a snake_case_name')).toEqual({ body: 'a snake_case_name', html: null });
  });

  it('should keep unmatched markers as typed', () => {
    expect(outgoingText('2 ** 3 and a * b')).toEqual({ body: '2 ** 3 and a * b', html: null });
  });

  it('should not format markers around spaces', () => {
    expect(outgoingText('** loose **').html).toBeNull();
  });

  it('should take escaped markers literally', () => {
    expect(outgoingText('\\*not italic\\*')).toEqual({ body: '*not italic*', html: null });
  });

  it('should escape html in the text', () => {
    expect(outgoingText('**<b>&"**').html).toBe('<p><strong>&lt;b&gt;&amp;&quot;</strong></p>');
  });

  it('should give every line its own paragraph once formatted', () => {
    expect(outgoingText('**a**\nb').html).toBe('<p><strong>a</strong></p><p>b</p>');
  });

  it('should send a bulleted list without its markers in the body', () => {
    expect(outgoingText('- one\n- two')).toEqual({ body: 'one\ntwo', html: '<ul><li>one</li><li>two</li></ul>' });
  });

  it('should send a numbered list', () => {
    expect(outgoingText('1. one\n2. two').html).toBe('<ol><li>one</li><li>two</li></ol>');
  });

  it('should nest an indented item in the list above', () => {
    expect(outgoingText('- one\n  - inner\n- two').html).toBe('<ul><li>one</li><ul><li>inner</li></ul><li>two</li></ul>');
  });

  it('should switch lists when the kind changes', () => {
    expect(outgoingText('- a\n1. b').html).toBe('<ul><li>a</li></ul><ol><li>b</li></ol>');
  });

  it('should send each quoted line as a quote', () => {
    expect(outgoingText('> a\n> b').html).toBe('<blockquote>a</blockquote><blockquote>b</blockquote>');
  });

  it('should send a fenced code block as one pre without formatting inside', () => {
    expect(outgoingText('```\nlet **a** = 1;\nx < y\n```')).toEqual({
      body: 'let **a** = 1;\nx < y',
      html: '<pre><code>let **a** = 1;\nx &lt; y</code></pre>',
    });
  });

  it('should send an unclosed fence as code to the end', () => {
    expect(outgoingText('```\ncode').html).toBe('<pre><code>code</code></pre>');
  });

  it('should mark directions when a block reads right to left', () => {
    expect(outgoingText('**שלום**\nhi').html).toBe('<p dir="rtl"><strong>שלום</strong></p><p dir="ltr">hi</p>');
  });

  it('should send nothing for an empty draft', () => {
    expect(outgoingText(' \n ')).toEqual({ body: '', html: null });
  });
});

describe('applyFormat', () => {
  it('should wrap a selection in bold markers and keep it selected', () => {
    expect(applyFormat('one two', { start: 4, end: 7 }, 'bold')).toEqual({ text: 'one **two**', selection: { start: 6, end: 9 } });
  });

  it('should unwrap a selection that is bold already', () => {
    expect(applyFormat('one **two**', { start: 6, end: 9 }, 'bold')).toEqual({ text: 'one two', selection: { start: 4, end: 7 } });
  });

  it('should leave a space selected after a word outside the markers', () => {
    expect(applyFormat('word next', { start: 0, end: 5 }, 'italic').text).toBe('_word_ next');
  });

  it('should insert a pair with the caret between when nothing is selected', () => {
    expect(applyFormat('a ', caret(2), 'strike')).toEqual({ text: 'a ~~~~', selection: caret(4) });
  });

  it('should take an empty pair away when toggled again', () => {
    expect(applyFormat('a ``', caret(3), 'code')).toEqual({ text: 'a ', selection: caret(2) });
  });

  it('should make every selected line a bullet', () => {
    expect(applyFormat('one\ntwo', { start: 0, end: 7 }, 'bullets').text).toBe('- one\n- two');
  });

  it('should number the selected lines in order', () => {
    expect(applyFormat('one\ntwo', { start: 0, end: 7 }, 'numbers').text).toBe('1. one\n2. two');
  });

  it('should take the bullets off lines that all have them', () => {
    expect(applyFormat('- one\n- two', { start: 0, end: 11 }, 'bullets').text).toBe('one\ntwo');
  });

  it('should turn a bullet into a number', () => {
    expect(applyFormat('- one', caret(3), 'numbers').text).toBe('1. one');
  });

  it('should quote the caret line only', () => {
    expect(applyFormat('a\nb\nc', caret(2), 'quote').text).toBe('a\n> b\nc');
  });

  it('should keep the caret on its word when a prefix is added', () => {
    expect(applyFormat('one', caret(3), 'bullets').selection).toEqual(caret(5));
  });

  it('should fence the selected lines as a code block', () => {
    expect(applyFormat('x\ncode', caret(4), 'codeBlock').text).toBe('x\n```\ncode\n```');
  });

  it('should take the fences away when toggled inside them', () => {
    expect(applyFormat('```\ncode\n```', caret(5), 'codeBlock').text).toBe('code');
  });
});

describe('applyLink', () => {
  it('should link the selection to the address', () => {
    expect(applyLink('the blog', { start: 4, end: 8 }, 'https://matrix.org')).toEqual({
      text: 'the [blog](https://matrix.org)',
      selection: caret(30),
    });
  });

  it('should put the address in as its own label when nothing is selected', () => {
    expect(applyLink('', caret(0), ' https://a.b ').text).toBe('[https://a.b](https://a.b)');
  });

  it('should leave the text alone without an address', () => {
    expect(applyLink('x', caret(1), 'https://').text).toBe('x');
  });
});

describe('continueBlock', () => {
  it('should start the next bullet after a line break in a list', () => {
    expect(continueBlock('- one', '- one\n')).toEqual({ text: '- one\n- ', selection: caret(8) });
  });

  it('should count on in a numbered list', () => {
    expect(continueBlock('  3. c', '  3. c\n')?.text).toBe('  3. c\n  4. ');
  });

  it('should continue a quote', () => {
    expect(continueBlock('> a', '> a\n')?.text).toBe('> a\n> ');
  });

  it('should end the list on an empty item', () => {
    expect(continueBlock('- a\n- ', '- a\n- \n')).toEqual({ text: '- a\n', selection: caret(4) });
  });

  it('should leave a break in a plain line alone', () => {
    expect(continueBlock('plain', 'plain\n')).toBeNull();
  });

  it('should leave other typing alone', () => {
    expect(continueBlock('- a', '- ab')).toBeNull();
  });

  it('should not continue inside a code block', () => {
    expect(continueBlock('```\n- a', '```\n- a\n')).toBeNull();
  });
});

describe('activeBlocks', () => {
  it('should show the list the caret is in', () => {
    expect(activeBlocks('a\n- b', 4)).toEqual(['bullets']);
  });

  it('should show a code block around the caret', () => {
    expect(activeBlocks('```\ncode', 6)).toEqual(['codeBlock']);
  });
});

describe('draftOf', () => {
  it('should give the body of a message without html', () => {
    expect(draftOf('plain', null)).toBe('plain');
  });

  it('should write marks back as markers', () => {
    expect(draftOf('a b', '<p>a <strong>b</strong> <em>c</em> <s>d</s> <code>e</code></p>')).toBe('a **b** _c_ ~~d~~ `e`');
  });

  it('should write a link back with its address', () => {
    expect(draftOf('x', '<a href="https://a.b/?q=1&amp;r=2">x</a>')).toBe('[x](https://a.b/?q=1&r=2)');
  });

  it('should write lists back as items', () => {
    expect(draftOf('', '<ul>\n<li>a</li>\n<li>b<ol><li>c</li></ol></li>\n</ul>')).toBe('- a\n- b\n  1. c');
  });

  it('should write quotes and code blocks back', () => {
    expect(draftOf('', '<blockquote>q</blockquote><pre><code>x &lt; y\nz</code></pre>')).toBe('> q\n```\nx < y\nz\n```');
  });

  it('should drop a reply fallback', () => {
    expect(draftOf('', '<mx-reply><blockquote>old</blockquote></mx-reply>new')).toBe('new');
  });

  it('should send back what the desktop sent for the same draft', () => {
    const draft = '**loud** and [link](https://x.y)\n- one\n- two';
    const { body, html } = outgoingText(draft);
    expect(draftOf(body, html)).toBe(draft);
  });
});
