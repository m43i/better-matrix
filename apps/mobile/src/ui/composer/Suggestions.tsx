// What the composer suggests for the word being typed, listed above the field as Slack's mobile
// composer lists them: people (face, name, user ID), `@room`, rooms, custom and standard emoji,
// and commands with what they take and do. Tapping one completes the word.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { Image } from 'expo-image';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import type { Suggestion } from '../../state/suggestions';
import { font } from '../../theme/palette';
import { Avatar } from '../Avatar';
import { EmojiText } from '../emoji/EmojiText';
import { Icon } from '../icons';
import { AVATAR_SIZE, useMedia } from '../media';
import { usePalette } from '../theme';

/** How tall the list grows before it scrolls: about four rows. */
const MAX_HEIGHT = 4 * 48;

export function Suggestions({
  account,
  suggestions,
  onPick,
}: {
  account: AccountId;
  suggestions: readonly Suggestion[];
  onPick: (suggestion: Suggestion) => void;
}) {
  const palette = usePalette();

  if (suggestions.length === 0) return null;

  return (
    <ScrollView
      testID="composer-suggestions"
      accessibilityLabel={t('mobile.composer.suggestions')}
      keyboardShouldPersistTaps="always"
      style={[styles.list, { borderBottomColor: palette.divider }]}
    >
      {suggestions.map((suggestion) => (
        <Pressable
          key={suggestion.key}
          testID={`suggestion-${suggestion.key}`}
          accessibilityRole="button"
          onPress={() => onPick(suggestion)}
          android_ripple={{ color: palette.hover }}
          style={({ pressed }) => [
            styles.row,
            { backgroundColor: pressed ? palette.hover : 'transparent' },
          ]}
        >
          <SuggestionRow account={account} suggestion={suggestion} />
        </Pressable>
      ))}
    </ScrollView>
  );
}

function SuggestionRow({ account, suggestion }: { account: AccountId; suggestion: Suggestion }) {
  const palette = usePalette();

  switch (suggestion.type) {
    case 'pill':
      return (
        <>
          {suggestion.face ? (
            <Avatar
              account={account}
              id={suggestion.face.id}
              name={suggestion.face.name}
              media={suggestion.face.media}
              size={28}
            />
          ) : (
            <View style={[styles.glyph, { backgroundColor: palette.surface }]}>
              <Icon name="at" size={18} color={palette.mention} />
            </View>
          )}
          <Text style={[font.bodyStrong, styles.label, { color: palette.text }]} numberOfLines={1}>
            {suggestion.pill.label}
          </Text>
          <Text style={[font.meta, styles.detail, { color: palette.textMuted }]} numberOfLines={1}>
            {suggestion.detail ?? (suggestion.face ? '' : t('mobile.composer.mention_everyone'))}
          </Text>
        </>
      );
    case 'emote':
      return (
        <>
          <PackPicture account={account} media={suggestion.image.media} />
          <Text style={[font.body, styles.label, { color: palette.text }]} numberOfLines={1}>
            {suggestion.pill.label}
          </Text>
        </>
      );
    case 'emoji':
      return (
        <>
          <View style={styles.glyph}>
            <EmojiText style={styles.emoji}>{suggestion.emoji}</EmojiText>
          </View>
          <Text style={[font.body, styles.label, { color: palette.text }]} numberOfLines={1}>
            {`:${suggestion.shortcode}:`}
          </Text>
        </>
      );
    case 'command':
      return (
        <>
          <View style={[styles.glyph, { backgroundColor: palette.surface }]}>
            <Icon name="command" size={16} color={palette.textMuted} />
          </View>
          <Text style={[font.bodyStrong, { color: palette.text }]} numberOfLines={1}>
            {`/${suggestion.command.name}`}
            {suggestion.command.args ? (
              <Text style={[font.body, { color: palette.textMuted }]}>
                {` ${t(suggestion.command.args)}`}
              </Text>
            ) : null}
          </Text>
          <Text style={[font.meta, styles.detail, { color: palette.textMuted }]} numberOfLines={1}>
            {t(suggestion.command.about)}
          </Text>
        </>
      );
  }
}

/** A pack's image, small. */
export function PackPicture({
  account,
  media,
  size = 28,
}: {
  account: AccountId;
  media: string;
  size?: number;
}) {
  const uri = useMedia(account, media, AVATAR_SIZE);

  return (
    <View style={{ width: size, height: size }}>
      {uri ? <Image source={{ uri }} style={StyleSheet.absoluteFill} contentFit="contain" /> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { maxHeight: MAX_HEIGHT, borderBottomWidth: StyleSheet.hairlineWidth, marginBottom: 4 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    minHeight: 44,
    paddingHorizontal: 14,
    paddingVertical: 6,
  },
  glyph: {
    width: 28,
    height: 28,
    borderRadius: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  emoji: { fontSize: 20 },
  label: { flexShrink: 1 },
  detail: { flex: 1 },
});
