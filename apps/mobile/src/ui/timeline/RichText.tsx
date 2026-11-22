// A message's text, drawn from its parsed blocks (state/html.ts) with native text: formatting,
// links that open in the browser, mention pills, spoilers hidden until tapped, formulas as
// Unicode, custom emoji as inline pictures, code, quotes and lists. Never raw HTML.

import { memo, useState } from 'react';
import { Image, Platform, ScrollView, StyleSheet, Text, type TextStyle, View } from 'react-native';

import { t } from '../../i18n';
import type { Block, Inline, Marks } from '../../state/html';
import { mxcMedia } from '../../state/messages';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { AVATAR_SIZE, useMedia } from '../media';
import { usePalette } from '../theme';
import { useTimelineContext } from './context';
import { openLink, openMention } from './open';

const MONO = Platform.select({ ios: 'Menlo', default: 'monospace' });

export const RichText = memo(function RichText({
  blocks,
  muted = false,
}: {
  blocks: Block[];
  muted?: boolean;
}) {
  return (
    <View style={styles.blocks}>
      {blocks.map((block, index) => (
        <BlockView key={index} block={block} muted={muted} />
      ))}
    </View>
  );
});

function BlockView({ block, muted }: { block: Block; muted: boolean }) {
  const palette = usePalette();
  const color = muted ? palette.textMuted : palette.text;

  switch (block.type) {
    case 'paragraph':
      return (
        <Text style={[font.body, { color }]}>
          <Inlines inlines={block.inlines} />
        </Text>
      );
    case 'heading':
      return (
        <Text
          style={[
            font.body,
            styles.heading,
            { color, fontSize: block.level === 1 ? 20 : block.level === 2 ? 18 : 16 },
          ]}
        >
          <Inlines inlines={block.inlines} />
        </Text>
      );
    case 'code':
      return (
        <ScrollView
          horizontal
          style={[
            styles.codeBlock,
            { backgroundColor: palette.surface, borderColor: palette.divider },
          ]}
          contentContainerStyle={styles.codeContent}
        >
          <Text style={[styles.mono, { color: palette.text }]}>{block.text}</Text>
        </ScrollView>
      );
    case 'quote':
      return (
        <View style={[styles.quote, { borderLeftColor: palette.divider }]}>
          <RichText blocks={block.blocks} muted />
        </View>
      );
    case 'list':
      return (
        <View style={styles.list}>
          {block.items.map((item, index) => (
            <View key={index} style={styles.item}>
              <Text style={[font.body, styles.bullet, { color: palette.textMuted }]}>
                {block.ordered ? `${block.start + index}.` : '•'}
              </Text>
              <View style={styles.itemBody}>
                <RichText blocks={item} muted={muted} />
              </View>
            </View>
          ))}
        </View>
      );
    case 'rule':
      return <View style={[styles.rule, { backgroundColor: palette.divider }]} />;
  }
}

function Inlines({ inlines }: { inlines: Inline[] }) {
  return (
    <>
      {inlines.map((inline, index) => {
        switch (inline.type) {
          case 'text':
            return <Run key={index} text={inline.text} marks={inline.marks} />;
          case 'mention':
            return <Pill key={index} inline={inline} />;
          case 'spoiler':
            return <Spoiler key={index} inlines={inline.inlines} />;
          case 'math':
            return <MathText key={index} text={inline.text} latex={inline.latex} />;
          case 'emote':
            return <Emote key={index} url={inline.url} label={inline.label} />;
        }
      })}
    </>
  );
}

/** Hidden text: a block of the muted colour until tapped, then the text as sent. */
function Spoiler({ inlines }: { inlines: Inline[] }) {
  const palette = usePalette();
  const [shown, setShown] = useState(false);

  if (shown) return <Inlines inlines={inlines} />;

  return (
    <Text
      accessibilityRole="button"
      accessibilityLabel={t('mobile.conversation.spoiler')}
      onPress={() => setShown(true)}
      suppressHighlighting
      style={{ color: palette.textMuted, backgroundColor: palette.textMuted }}
    >
      <Inlines inlines={inlines} />
    </Text>
  );
}

/** A formula as Unicode, set like maths; its LaTeX is what a screen reader says. */
function MathText({ text, latex }: { text: string; latex: string }) {
  return (
    <Text accessibilityLabel={latex} style={styles.math}>
      {text}
    </Text>
  );
}

/** How tall custom emoji are drawn: the body's line. */
const EMOTE = 20;

/** A custom emoji, as a picture in the line once fetched; its short code until then. */
function Emote({ url, label }: { url: string; label: string }) {
  const { account } = useTimelineContext();
  const uri = useMedia(account, mxcMedia(url), AVATAR_SIZE);

  if (!uri) return <Text>{label}</Text>;

  return (
    <Image
      source={{ uri }}
      accessibilityLabel={t('mobile.conversation.custom_emoji', { code: label })}
      style={styles.emote}
      resizeMode="contain"
    />
  );
}

function Run({ text, marks }: { text: string; marks: Marks }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account } = useTimelineContext();

  const underline = marks.underline || marks.link;

  const decoration = underline
    ? marks.strike
      ? 'underline line-through'
      : 'underline'
    : marks.strike
      ? 'line-through'
      : undefined;

  const style: TextStyle = {
    fontWeight: marks.bold ? '700' : undefined,
    fontStyle: marks.italic ? 'italic' : undefined,
    textDecorationLine: decoration || undefined,
    color: marks.link ? palette.accent : undefined,
    ...(marks.code ? { fontFamily: MONO, fontSize: 14, backgroundColor: palette.surface } : null),
  };

  const link = marks.link;

  if (link) {
    return (
      <Text style={style} onPress={() => openLink(store, link, account)} accessibilityRole="link">
        {text}
      </Text>
    );
  }

  return <Text style={style}>{text}</Text>;
}

/** A person or room, as a pill; the user's own name stands out. */
function Pill({ inline }: { inline: Extract<Inline, { type: 'mention' }> }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const me = inline.kind === 'user' && inline.id === account;

  const label =
    inline.kind === 'user' && !inline.label.startsWith('@') ? `@${inline.label}` : inline.label;

  return (
    <Text
      accessibilityRole="link"
      onPress={() => openMention(store, account, timelineKey.room_id, inline)}
      style={{
        fontWeight: '600',
        color: me ? palette.mention : palette.accent,
        backgroundColor: me ? `${palette.mention}26` : `${palette.accent}1f`,
      }}
    >
      {` ${label} `}
    </Text>
  );
}

const styles = StyleSheet.create({
  blocks: { gap: 4 },
  heading: { fontWeight: '700' },
  codeBlock: { borderRadius: 6, borderWidth: StyleSheet.hairlineWidth },
  codeContent: { padding: 8 },
  mono: { fontFamily: MONO, fontSize: 13, lineHeight: 18 },
  quote: { borderLeftWidth: 3, paddingLeft: 10 },
  list: { gap: 2 },
  item: { flexDirection: 'row', gap: 6 },
  bullet: { minWidth: 16, textAlign: 'right' },
  itemBody: { flex: 1 },
  rule: { height: StyleSheet.hairlineWidth, marginVertical: 6 },
  math: { fontFamily: Platform.select({ ios: 'Times New Roman', default: 'serif' }), fontSize: 17 },
  emote: { width: EMOTE, height: EMOTE },
});
