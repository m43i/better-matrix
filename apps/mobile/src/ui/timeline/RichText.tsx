// A message's text, drawn from its parsed blocks (state/html.ts) with native text: formatting,
// links that open in the browser, mention pills, spoilers hidden until tapped, formulas as
// Unicode, custom emoji as inline pictures, code, quotes and lists. Never raw HTML. Inline
// pictures follow the message's media (ADR 0011): held back, each is its short code, fetched
// only once the user shows the message's media (a tap on the code does).

import { createContext, memo, useContext } from 'react';
import {
  Image,
  Platform,
  ScrollView,
  type StyleProp,
  StyleSheet,
  Text,
  type TextStyle,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { type Block, type Inline, type Marks } from '../../state/html';
import { revealMedia } from '../../state/actions/safety';
import { useAppStore, useStore } from '../../state/react';
import { inlinePicture } from '../../state/selectors';
import { hiddenReading, inlineSpoilerId, spoilerCover } from '../../state/spoilers';
import { font } from '../../theme/palette';
import { EmojiText } from '../emoji/EmojiText';
import { AVATAR_SIZE, useMedia } from '../media';
import { usePalette } from '../theme';
import { SpoilerContext, useSpoiler, useTimelineContext } from './context';
import { openLink, openMention } from './open';

const MONO = Platform.select({ ios: 'Menlo', default: 'monospace' });

/** The message whose media choice an inline picture follows: its sender and event. */
export interface MediaOwner {
  sender_id: string;
  event_id: string;
}

/** The message the text being drawn belongs to, for its inline pictures; none fetches nothing. */
const MediaOwnerContext = createContext<MediaOwner | null>(null);

/**
 * `blocks` of the message `owner` (its event ID: a spoiler shown in it stays shown for it only).
 * `media` is the message whose media choice its inline pictures follow, given where the text
 * starts; blocks inside a quote or a list keep their message's. `place` is where these blocks
 * stand in the message, for blocks inside a quote or a list.
 */
export const RichText = memo(function RichText({
  blocks,
  owner,
  media,
  muted = false,
  place = '',
}: {
  blocks: Block[];
  owner: string;
  media?: MediaOwner;
  muted?: boolean;
  place?: string;
}) {
  const view = (
    <View style={styles.blocks}>
      {blocks.map((block, index) => (
        <BlockView
          key={index}
          block={block}
          owner={owner}
          muted={muted}
          place={`${place}${index}`}
        />
      ))}
    </View>
  );

  return media ? (
    <MediaOwnerContext.Provider value={media}>{view}</MediaOwnerContext.Provider>
  ) : (
    view
  );
});

interface Placed {
  owner: string;
  /** The path to this block or inline through the message's blocks. */
  place: string;
}

function BlockView({ block, muted, owner, place }: { block: Block; muted: boolean } & Placed) {
  const palette = usePalette();
  const color = muted ? palette.textMuted : palette.text;

  switch (block.type) {
    case 'paragraph':
      return (
        <Line style={[font.body, { color }]} inlines={block.inlines} owner={owner} place={place} />
      );
    case 'heading':
      return (
        <Line
          style={[
            font.body,
            styles.heading,
            { color, fontSize: block.level === 1 ? 20 : block.level === 2 ? 18 : 16 },
          ]}
          inlines={block.inlines}
          owner={owner}
          place={place}
        />
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
          <EmojiText style={[styles.mono, { color: palette.text }]}>{block.text}</EmojiText>
        </ScrollView>
      );
    case 'quote':
      return (
        <View style={[styles.quote, { borderLeftColor: palette.divider }]}>
          <RichText blocks={block.blocks} owner={owner} muted place={`${place}.`} />
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
                <RichText blocks={item} owner={owner} muted={muted} place={`${place}.${index}.`} />
              </View>
            </View>
          ))}
        </View>
      );
    case 'rule':
      return <View style={[styles.rule, { backgroundColor: palette.divider }]} />;
  }
}

interface LineProps extends Placed {
  inlines: Inline[];
  style: StyleProp<TextStyle>;
}

/** A paragraph or heading: one native text holding its inlines. */
function Line(props: LineProps) {
  return props.inlines.some((inline) => inline.type === 'spoiler') ? (
    <SpoiledLine {...props} />
  ) : (
    <Text style={props.style}>
      <Inlines inlines={props.inlines} owner={props.owner} place={props.place} />
    </Text>
  );
}

/**
 * A line holding a spoiler: while one is hidden, it reads each hidden one as the spoiler's
 * label, not as its blank cover. Only such lines follow the spoilers shown, so a tap redraws them.
 */
function SpoiledLine({ inlines, owner, place, style }: LineProps) {
  const { revealed } = useContext(SpoilerContext);
  const reading = hiddenReading(inlines, owner, place, revealed, t('mobile.conversation.spoiler'));

  return (
    <Text style={style} accessibilityLabel={reading ?? undefined}>
      <Inlines inlines={inlines} owner={owner} place={place} />
    </Text>
  );
}

function Inlines({ inlines, owner, place }: { inlines: Inline[] } & Placed) {
  return (
    <>
      {inlines.map((inline, index) => {
        switch (inline.type) {
          case 'text':
            return <Run key={index} text={inline.text} marks={inline.marks} />;
          case 'mention':
            return <Pill key={index} inline={inline} />;
          case 'spoiler':
            return (
              <Spoiler key={index} id={inlineSpoilerId(owner, place, index, inline.text)}>
                {inline.text}
              </Spoiler>
            );
          case 'math':
            return <MathText key={index} text={inline.text} latex={inline.latex} />;
          case 'emote':
            return <Emote key={index} url={inline.url} label={inline.label} />;
        }
      })}
    </>
  );
}

/**
 * Hidden text: a block of the muted colour until tapped, then its words. While hidden its words
 * are not in the drawn text at all, only a blank cover as wide (`spoilerCover`): a screen reader
 * reads a paragraph's text whatever its colours. A spoiler is only ever
 * its plain words (`state/html.ts`): no link, mention or custom emoji inside it shows anything,
 * hidden or not. Whether it is shown is the screen's, by the spoiler's `id` (its message, place
 * and words, `state/spoilers.ts`), not this view's: a recycled row or an edit shows it hidden.
 */
function Spoiler({ id, children }: { id: string; children: string }) {
  const palette = usePalette();
  const { shown, reveal } = useSpoiler(id);

  if (shown) return <EmojiText>{children}</EmojiText>;

  return (
    <Text
      accessibilityRole="button"
      accessibilityLabel={t('mobile.conversation.spoiler')}
      onPress={reveal}
      suppressHighlighting
      style={{ color: palette.textMuted, backgroundColor: palette.textMuted }}
    >
      {spoilerCover(children)}
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

/**
 * A custom emoji, as a picture in the line once fetched; its short code until then, and while
 * its message's media is held back, when a tap on the code shows the message's media.
 */
function Emote({ url, label }: { url: string; label: string }) {
  const store = useAppStore();
  const { account, timelineKey } = useTimelineContext();
  const owner = useContext(MediaOwnerContext);

  const media = useStore((s) =>
    owner ? inlinePicture(s, account, timelineKey.room_id, owner, url) : null,
  );

  const uri = useMedia(account, media, AVATAR_SIZE);

  if (!media) {
    return (
      <Text
        accessibilityRole={owner ? 'button' : undefined}
        accessibilityLabel={
          owner
            ? `${t('mobile.conversation.custom_emoji', { code: label })}. ${t('safety.show_media')}`
            : undefined
        }
        onPress={owner ? () => revealMedia(store, account, owner.event_id) : undefined}
        suppressHighlighting
      >
        {label}
      </Text>
    );
  }

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
  const { account, shown } = useTimelineContext();

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
      <EmojiText
        style={style}
        onPress={() => openLink(store, link, account, shown)}
        accessibilityRole="link"
      >
        {text}
      </EmojiText>
    );
  }

  return <EmojiText style={style}>{text}</EmojiText>;
}

/** A person or room, as a pill; the user's own name stands out. */
function Pill({ inline }: { inline: Extract<Inline, { type: 'mention' }> }) {
  const palette = usePalette();
  const store = useAppStore();
  const { account, timelineKey, shown } = useTimelineContext();
  const me = inline.kind === 'user' && inline.id === account;

  const label =
    inline.kind === 'user' && !inline.label.startsWith('@') ? `@${inline.label}` : inline.label;

  return (
    <EmojiText
      accessibilityRole="link"
      onPress={() => openMention(store, account, timelineKey.room_id, inline, shown)}
      style={{
        fontWeight: '600',
        color: me ? palette.mention : palette.accent,
        backgroundColor: me ? `${palette.mention}26` : `${palette.accent}1f`,
      }}
    >
      {` ${label} `}
    </EmojiText>
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
