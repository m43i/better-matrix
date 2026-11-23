// The emoji picker: a search over every emoji by name and shortcode, else the recently used (or
// the quick reactions, before any was used) and every emoji by group, with a tab per group to
// jump to it. One picker serves reactions and drafts; the route says where a pick goes. For a
// draft, the custom emoji of the room's image packs come first (`custom`).

import type { AccountId, PackImage } from '@better-matrix/matrix-native/types';
import { FlashList, type FlashListRef, type ViewToken } from '@shopify/flash-list';
import { useMemo, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, useWindowDimensions, View } from 'react-native';

import { type Key, t } from '../../i18n';
import {
  type Emoji,
  emojiByGroup,
  emojiLabel,
  type Group,
  GROUPS,
  QUICK,
  searchEmoji,
} from '../../state/emoji';
import { shallow, useStore } from '../../state/react';
import { font } from '../../theme/palette';
import { EmptyState } from '../components';
import { PackPicture } from '../composer/Suggestions';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { EmojiText } from './EmojiText';

type Section = 'custom' | 'recent' | Group;

const SECTIONS: Record<Section, { icon: IconName; title: Key }> = {
  custom: { icon: 'sticker', title: 'mobile.composer.custom_emoji' },
  recent: { icon: 'clock', title: 'mobile.composer.emoji_recent' },
  smileys: { icon: 'smile', title: 'emoji.smileys' },
  people: { icon: 'hand', title: 'emoji.people' },
  nature: { icon: 'leaf', title: 'emoji.nature' },
  food: { icon: 'food', title: 'emoji.food' },
  travel: { icon: 'car', title: 'mobile.composer.emoji_travel' },
  activities: { icon: 'ball', title: 'mobile.composer.emoji_activities' },
  objects: { icon: 'lightbulb', title: 'mobile.composer.emoji_objects' },
  symbols: { icon: 'heart', title: 'emoji.symbols' },
  flags: { icon: 'flag', title: 'mobile.composer.emoji_flags' },
};

/** One cell: a Unicode emoji, or a custom one (its short code as `emoji`, and its image). */
interface Cell {
  emoji: string;
  label: string;
  image?: PackImage;
}

type Item =
  | { type: 'header'; id: string; section: Section; title: string }
  | { type: 'row'; id: string; section: Section; emoji: Cell[] };

const CELL = 46;

function rows(section: Section, emoji: readonly Cell[], columns: number): Item[] {
  const items: Item[] = [];

  for (let i = 0; i < emoji.length; i += columns) {
    items.push({ type: 'row', id: `${section}-${i}`, section, emoji: emoji.slice(i, i + columns) });
  }

  return items;
}

function labelled(emoji: Emoji) {
  return { emoji: emoji.emoji, label: emoji.label };
}

function customCell(image: PackImage): Cell {
  return { emoji: `:${image.shortcode}:`, label: image.body, image };
}

export function EmojiPicker({
  onPick,
  custom,
}: {
  onPick: (emoji: string) => void;
  /** Custom emoji to offer first, and what picking one does. */
  custom?: { account: AccountId; images: readonly PackImage[]; onPick: (image: PackImage) => void };
}) {
  const palette = usePalette();
  const { width } = useWindowDimensions();
  const recent = useStore((s) => s.recentEmoji, shallow);
  const [query, setQuery] = useState('');
  const [current, setCurrent] = useState<Section>('recent');
  const list = useRef<FlashListRef<Item>>(null);
  const columns = Math.max(6, Math.floor((width - 16) / CELL));

  const groups = useMemo(() => emojiByGroup(), []);

  const { items, headers } = useMemo(() => {
    const all: Item[] = [];
    const at = new Map<Section, number>();

    const first = (recent.length > 0 ? recent : QUICK).map((emoji) => ({
      emoji,
      label: emojiLabel(emoji) ?? emoji,
    }));

    const customs = custom?.images ?? [];

    const sections: Array<{ section: Section; title: string; emoji: Cell[] }> = [
      ...(customs.length > 0
        ? [
            {
              section: 'custom' as const,
              title: t('mobile.composer.custom_emoji'),
              emoji: customs.map(customCell),
            },
          ]
        : []),
      {
        section: 'recent',
        title: recent.length > 0 ? t('mobile.composer.emoji_recent') : t('emoji.quick'),
        emoji: first,
      },
      ...groups.map((g) => ({
        section: g.group,
        title: t(SECTIONS[g.group].title),
        emoji: g.emoji.map(labelled),
      })),
    ];

    for (const { section, title, emoji } of sections) {
      at.set(section, all.length);
      all.push({ type: 'header', id: `header-${section}`, section, title });
      all.push(...rows(section, emoji, columns));
    }

    return { items: all, headers: at };
  }, [groups, recent, columns, custom?.images]);

  const results = useMemo(() => {
    const term = query.trim().toLowerCase();

    if (!term) return null;

    const customs = (custom?.images ?? [])
      .filter((image) => image.shortcode.toLowerCase().includes(term))
      .map(customCell);

    return [...customs, ...searchEmoji(query).map(labelled)];
  }, [query, custom?.images]);

  const choose = (emoji: Cell) => {
    if (emoji.image && custom) custom.onPick(emoji.image);
    else onPick(emoji.emoji);
  };

  const cell = (emoji: Cell, place: string) => (
    <Pressable
      key={`${place}-${emoji.emoji}`}
      testID={`emoji-${place}-${emoji.emoji}`}
      accessibilityRole="button"
      accessibilityLabel={emoji.label}
      onPress={() => choose(emoji)}
      android_ripple={{ color: palette.hover, borderless: true, radius: CELL / 2 }}
      style={({ pressed }) => [
        styles.cell,
        { backgroundColor: pressed ? palette.hover : 'transparent' },
      ]}
    >
      {emoji.image && custom ? (
        <PackPicture account={custom.account} media={emoji.image.media} size={30} />
      ) : (
        <EmojiText style={styles.glyph} allowFontScaling={false}>
          {emoji.emoji}
        </EmojiText>
      )}
    </Pressable>
  );

  const onViewable = ({ viewableItems }: { viewableItems: Array<ViewToken<Item>> }) => {
    const top = viewableItems[0]?.item;

    if (top) setCurrent(top.section);
  };

  const jump = (section: Section) => {
    const index = headers.get(section);
    setCurrent(section);

    if (index !== undefined) void list.current?.scrollToIndex({ index, animated: false });
  };

  return (
    <View style={[styles.picker, { backgroundColor: palette.bg }]}>
      <View
        style={[styles.search, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        <Icon name="search" size={18} color={palette.textMuted} />
        <TextInput
          testID="emoji-search"
          value={query}
          onChangeText={setQuery}
          placeholder={t('emoji.search')}
          accessibilityLabel={t('emoji.search')}
          placeholderTextColor={palette.textMuted}
          selectionColor={palette.accent}
          autoCapitalize="none"
          autoCorrect={false}
          returnKeyType="done"
          clearButtonMode="while-editing"
          // Enter picks the first match, as on the desktop.
          onSubmitEditing={() => {
            const first = results?.[0];

            if (first) choose(first);
          }}
          style={[font.body, styles.searchInput, { color: palette.text }]}
        />
      </View>
      {results ? (
        results.length === 0 ? (
          <EmptyState icon="search" title={t('emoji.none')} />
        ) : (
          <FlashList
            key={`results-${columns}`}
            data={rows('recent', results, columns)}
            keyExtractor={(item) => item.id}
            keyboardShouldPersistTaps="handled"
            renderItem={({ item }) =>
              item.type === 'row' ? (
                <View style={styles.row}>{item.emoji.map((e) => cell(e, 'found'))}</View>
              ) : null
            }
          />
        )
      ) : (
        <>
          <View style={[styles.tabs, { borderBottomColor: palette.divider }]}>
            {[
              ...((custom?.images.length ?? 0) > 0 ? (['custom'] as const) : []),
              ...(['recent', ...GROUPS] as const),
            ].map((section) => (
              <Pressable
                key={section}
                testID={`emoji-tab-${section}`}
                accessibilityRole="tab"
                accessibilityState={{ selected: current === section }}
                accessibilityLabel={t(SECTIONS[section].title)}
                onPress={() => jump(section)}
                hitSlop={4}
                style={[
                  styles.tab,
                  current === section ? { borderBottomColor: palette.accent } : null,
                ]}
              >
                <Icon
                  name={SECTIONS[section].icon}
                  size={18}
                  color={current === section ? palette.accent : palette.textMuted}
                />
              </Pressable>
            ))}
          </View>
          <FlashList
            ref={list}
            key={`groups-${columns}`}
            data={items}
            keyExtractor={(item) => item.id}
            getItemType={(item) => item.type}
            keyboardShouldPersistTaps="handled"
            onViewableItemsChanged={onViewable}
            renderItem={({ item }) =>
              item.type === 'header' ? (
                <Text style={[font.section, styles.header, { color: palette.textMuted }]}>
                  {item.title}
                </Text>
              ) : (
                <View style={styles.row}>{item.emoji.map((e) => cell(e, item.section))}</View>
              )
            }
          />
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  picker: { flex: 1, paddingTop: 16 },
  search: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    marginHorizontal: 12,
    paddingHorizontal: 10,
    borderRadius: 10,
    borderWidth: StyleSheet.hairlineWidth,
  },
  searchInput: { flex: 1, paddingVertical: 8 },
  tabs: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingHorizontal: 8,
    marginTop: 8,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  tab: {
    paddingVertical: 8,
    paddingHorizontal: 4,
    borderBottomWidth: 2,
    borderBottomColor: 'transparent',
  },
  header: { paddingHorizontal: 14, paddingTop: 12, paddingBottom: 4 },
  row: { flexDirection: 'row', paddingHorizontal: 8 },
  cell: {
    width: CELL,
    height: CELL,
    borderRadius: CELL / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  glyph: { fontSize: 28 },
});
