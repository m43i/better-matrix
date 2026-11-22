// An event's source (developer tools, from a message's actions or a hidden event): its JSON as
// this device reads it and, for an encrypted event, as it was sent. Selectable, and copied whole
// with one button.

import type { EventSource } from '@better-matrix/matrix-native/types';
import * as Clipboard from 'expo-clipboard';
import { useLocalSearchParams } from 'expo-router';
import { useEffect, useState } from 'react';
import { Platform, ScrollView, StyleSheet, Text, View } from 'react-native';

import { t } from '../i18n';
import { viewSource } from '../state/actions/developer';
import { useAppStore } from '../state/react';
import { font } from '../theme/palette';
import { EmptyState, ListRow, ListSection } from '../ui/components';
import { usePalette } from '../ui/theme';

const MONO = Platform.select({ ios: 'Menlo', default: 'monospace' });

export default function EventSourceScreen() {
  const { account, roomId, eventId } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
  }>();

  const store = useAppStore();
  const palette = usePalette();
  const [source, setSource] = useState<EventSource | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let active = true;
    viewSource(store, account, roomId, eventId).then(
      (found) => {
        if (active) setSource(found);
      },
      (failed: Error) => {
        if (active) setError(t('notice.source', { error: failed.message }));
      },
    );

    return () => {
      active = false;
    };
  }, [store, account, roomId, eventId]);

  if (error !== null) return <EmptyState icon="alert" title={error} />;

  if (source === null) return <EmptyState loading title={t('info.loading')} />;

  const block = (label: string, json: string) => (
    <View style={styles.block}>
      <Text style={[font.section, { color: palette.textMuted }]}>{label}</Text>
      <View
        style={[styles.code, { backgroundColor: palette.surface, borderColor: palette.divider }]}
      >
        <Text selectable style={[font.meta, { color: palette.text, fontFamily: MONO }]}>
          {json}
        </Text>
      </View>
    </View>
  );

  return (
    <ScrollView
      testID="event-source"
      style={{ backgroundColor: palette.bg }}
      contentContainerStyle={styles.sheet}
    >
      <Text style={[font.header, { color: palette.text }]}>{t('actions.view_source')}</Text>
      <Text selectable style={[font.meta, { color: palette.textMuted, fontFamily: MONO }]}>
        {t('source.ids', { room: source.room_id, event: source.event_id })}
      </Text>
      {source.encrypted === null ? (
        block(t('source.event'), source.json)
      ) : (
        <>
          {block(t('source.decrypted'), source.json)}
          {block(t('source.encrypted'), source.encrypted)}
        </>
      )}
      <ListSection>
        <ListRow
          testID="event-source-copy"
          icon="copy"
          title={copied ? t('source.copied') : t('source.copy')}
          chevron={false}
          onPress={() => {
            void Clipboard.setStringAsync(source.json);
            setCopied(true);
          }}
        />
      </ListSection>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  sheet: { padding: 16, gap: 14, paddingBottom: 40 },
  block: { gap: 6 },
  code: { padding: 12, borderRadius: 10, borderWidth: StyleSheet.hairlineWidth },
});
