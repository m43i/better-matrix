// A room's or space's settings (pushed from room info or a space's sheet): only the parts the user
// may change, each with its right (ADR 0008, Room settings). It asks for the settings when it
// opens; every change goes to the engine, which reports the room with it.

import { useLocalSearchParams, useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { t } from '../../../i18n';
import { routes } from '../../../routes';
import { upgradeRoom } from '../../../state/actions/admin';
import { settingsParts } from '../../../state/admin';
import { useAppStore } from '../../../state/react';
import { font } from '../../../theme/palette';
import {
  AccessPart,
  AddressesPart,
  AdvancedPart,
  GeneralPart,
  LinkPart,
  SecurityPart,
} from '../../../ui/admin/SettingsParts';
import { useRoomSettings } from '../../../ui/admin/useRoomSettings';
import { Avatar } from '../../../ui/Avatar';
import { EmptyState } from '../../../ui/components';
import { SettingsPage } from '../../../ui/settings/common';
import { usePalette } from '../../../ui/theme';
import { useShown } from '../../../ui/useShown';

export default function RoomSettingsScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const { settings, confirmed, change } = useRoomSettings(account, roomId);
  const shown = useShown();

  if (!settings) {
    return (
      <SettingsPage title={t('mobile.admin.settings')} testID="room-settings">
        <EmptyState title={t('mobile.admin.loading')} loading />
      </SettingsPage>
    );
  }

  const parts = settingsParts(settings);
  const name = settings.name ?? settings.main_address ?? roomId;

  const upgrade = (version: string) => {
    upgradeRoom(store, account, roomId, version).then(
      (next) => {
        // An upgrade that ends after the user went elsewhere leaves them there.
        if (!shown.current) return;
        router.dismissAll();
        router.push(routes.room(account, next));
      },
      () => {},
    );
  };

  return (
    <SettingsPage
      title={
        settings.space ? t('mobile.admin.space_settings_title') : t('mobile.admin.settings_title')
      }
      testID="room-settings"
    >
      <View style={styles.head}>
        <Avatar account={account} id={roomId} name={name} media={settings.avatar} size={72} />
        <Text style={[font.header, { color: palette.text }]} numberOfLines={1}>
          {name}
        </Text>
        {settings.main_address ? (
          <Text style={[font.meta, { color: palette.textMuted }]} selectable>
            {settings.main_address}
          </Text>
        ) : null}
      </View>
      {parts.map((part) => {
        switch (part) {
          case 'general':
            return (
              <GeneralPart
                key={part}
                settings={settings}
                confirmed={confirmed ?? settings}
                change={change}
              />
            );
          case 'access':
            return <AccessPart key={part} settings={settings} change={change} />;
          case 'addresses':
            return <AddressesPart key={part} settings={settings} change={change} />;
          case 'security':
            return <SecurityPart key={part} settings={settings} change={change} />;
          case 'permissions':
            return (
              <LinkPart
                key={part}
                testID="settings-permissions"
                icon="shield"
                title={t('mobile.admin.permissions')}
                onPress={() => router.push(routes.roomPermissions(account, roomId))}
              />
            );
          case 'rooms':
            return (
              <LinkPart
                key={part}
                testID="settings-space-rooms"
                icon="space"
                title={t('mobile.admin.manage_rooms')}
                onPress={() => router.push(routes.spaceRooms(account, roomId))}
              />
            );
          case 'advanced':
            return <AdvancedPart key={part} settings={settings} onUpgrade={upgrade} />;
        }
      })}
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  head: { alignItems: 'center', gap: 6, paddingTop: 20 },
});
