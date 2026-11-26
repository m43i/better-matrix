import { useLocalSearchParams } from 'expo-router';
import { Alert, Linking, Platform, ScrollView, StyleSheet, Text } from 'react-native';
import { t } from '../i18n';
import { liveLocations } from '../engine/locationTask';
import { shareKey } from '../engine/liveLocation';
import { keyOf } from '../state/actions/composer';
import { coordinatesText, navigationLink } from '../state/geo';
import { findMessage } from '../state/messages';
import { useAppStore, useStore } from '../state/react';
import { timeline } from '../state/selectors';
import { font } from '../theme/palette';
import { Button, EmptyState } from '../ui/components';
import { usePalette } from '../ui/theme';
import { useNow } from '../ui/shell/useNow';
import { MapView, senderPin, locationTitle } from '../ui/timeline/LocationCard';

export default function LocationMapScreen() {
  const { account, roomId, eventId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    eventId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const p = usePalette();

  const message = useStore((s) =>
    findMessage(timeline(s, account, keyOf(roomId, thread)).entries, eventId),
  );

  const share = useStore((s) => s.liveShares[shareKey(account, roomId)]);
  const location = message?.content.type === 'Location' ? message.content.data : undefined;
  const now = useNow();
  const title = locationTitle(location, now);

  if (!message || !location)
    return <EmptyState icon="info" title={t('mobile.conversation.message_missing')} />;
  const uri = navigationLink(location.geo_uri, Platform.OS);

  const open = async () => {
    if (!uri) return;

    try {
      await Linking.openURL(uri);
    } catch (error) {
      // Older devices have no default-navigation handler.
      if (Platform.OS === 'ios') {
        const point = coordinatesText(location.geo_uri)?.replace(' ', '');

        try {
          await Linking.openURL(`https://maps.apple.com/?ll=${point}&q=${point}`);

          return;
        } catch (fallbackError) {
          Alert.alert(t('location.navigate'), String(fallbackError));

          return;
        }
      }

      Alert.alert(t('location.navigate'), String(error));
    }
  };

  return (
    <ScrollView
      style={{ backgroundColor: p.bg }}
      contentContainerStyle={styles.content}
      testID="location-map"
    >
      <Text style={[font.header, { color: p.text }]}>{title}</Text>
      <MapView interactive account={account} uri={location.geo_uri} person={senderPin(message)} />
      <Text style={[font.meta, { color: p.textMuted }]}>
        {coordinatesText(location.geo_uri) ?? t('location.waiting')}
      </Text>
      {uri && <Button title={t('location.navigate')} onPress={open} testID="location-navigate" />}
      {message.sender_id === account &&
        (location.live_until ?? 0) > now &&
        share?.eventId === eventId && (
          <Button
            title={t('location.stop')}
            onPress={() => liveLocations.stop(store, share.request)}
            testID="location-stop"
          />
        )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({ content: { padding: 16, gap: 12 } });
