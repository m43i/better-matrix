// A room's or space's permissions (pushed from its settings): each one with the level it asks
// for. A permission asking no more than the user's own level opens its choices (Admin,
// Moderator, Member, or a custom number up to their own); those above it are read-only.

import type { Permission } from '@better-matrix/matrix-native/types';
import { useLocalSearchParams } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { t } from '../i18n';
import {
  choicesOffered,
  levelChoice,
  parseLevel,
  permissionEditable,
  permissionSetting,
} from '../state/admin';
import { usePrompt } from '../ui/accounts/Prompt';
import { levelLabel, permissionLabel } from '../ui/admin/labels';
import { useRoomSettings } from '../ui/admin/useRoomSettings';
import { Divider, EmptyState, ListRow, ListSection } from '../ui/components';
import { SettingsPage } from '../ui/settings/common';

export default function RoomPermissionsScreen() {
  const { account, roomId } = useLocalSearchParams<{ account: string; roomId: string }>();
  const { settings, change } = useRoomSettings(account, roomId);
  const prompt = usePrompt();
  const [open, setOpen] = useState<Permission | null>(null);

  if (!settings) {
    return (
      <SettingsPage title={t('mobile.admin.permissions')} testID="room-permissions">
        <EmptyState title={t('mobile.admin.loading')} loading />
      </SettingsPage>
    );
  }

  const own = settings.own_level;

  const set = (permission: Permission, level: number) => {
    setOpen(null);
    const setting = permissionSetting(settings, permission, level);

    if (setting) change(setting).catch(() => {});
  };

  const askCustom = async (permission: Permission, current: number) => {
    let error: string | null = null;

    for (;;) {
      const typed = await prompt.ask({
        title: t('mobile.admin.level.prompt_title'),
        message: t('mobile.admin.level.prompt_body', { level: own }),
        error,
        confirm: t('mobile.admin.level.set'),
        defaultValue: String(current),
      });

      if (typed === null) return;
      const level = parseLevel(typed, own);

      if (level !== null) {
        set(permission, level);

        return;
      }

      error = t('mobile.admin.level.invalid');
    }
  };

  return (
    <SettingsPage title={t('mobile.admin.permissions')} testID="room-permissions">
      <ListSection footer={t('mobile.admin.permissions_hint', { level: own })}>
        {settings.permissions.map((p, index) => {
          const editable = settings.rights.permissions && permissionEditable(p, own);
          const expanded = editable && open === p.permission;

          return (
            <View key={p.permission}>
              {index > 0 ? <Divider inset={16} /> : null}
              <ListRow
                testID={`permission-${p.permission}`}
                title={permissionLabel(p.permission)}
                value={levelLabel(p.level)}
                chevron={false}
                onPress={
                  editable
                    ? () => setOpen((current) => (current === p.permission ? null : p.permission))
                    : undefined
                }
              />
              {expanded ? (
                <>
                  {choicesOffered(own).map(([choice, level]) => (
                    <ListRow
                      key={choice}
                      testID={`permission-${p.permission}-${choice}`}
                      title={levelLabel(level)}
                      leading={<View style={styles.indent} />}
                      selected={p.level === level}
                      chevron={false}
                      onPress={() => set(p.permission, level)}
                    />
                  ))}
                  <ListRow
                    testID={`permission-${p.permission}-Custom`}
                    title={t('mobile.admin.level.choose_custom')}
                    leading={<View style={styles.indent} />}
                    selected={levelChoice(p.level) === 'Custom'}
                    chevron={false}
                    onPress={() => void askCustom(p.permission, p.level)}
                  />
                </>
              ) : null}
            </View>
          );
        })}
      </ListSection>
      {prompt.element}
    </SettingsPage>
  );
}

const styles = StyleSheet.create({
  indent: { width: 16 },
});
