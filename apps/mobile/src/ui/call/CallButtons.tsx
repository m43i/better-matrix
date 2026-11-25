// The voice and video call buttons in a room's header (the desktop's `header_buttons`): Join while
// a call is under way in the room, back to the stage while the user is in it. The video button is
// also the room's meeting control, as on the desktop (`videoOptions`; beside Join or Return it
// offers the meetings only): a tap runs the default
// (the video call where calls are available, else joining the pinned meeting, else starting
// one); a long press lists every option in the platform's chooser (an action sheet on iOS, an
// alert on Android, which holds three: past that the last opens the room's widgets, where every
// meeting is listed). No option, no video button.

import type { AccountId } from '@better-matrix/matrix-native/types';
import { useRouter } from 'expo-router';
import { useMemo } from 'react';
import {
  ActionSheetIOS,
  Alert,
  type AlertButton,
  Platform,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { routes } from '../../routes';
import { startCall } from '../../state/actions/calls';
import { shallow, useAppStore, useStore } from '../../state/react';
import { room as roomOf, roomMeetings } from '../../state/selectors';
import { type VideoOption, videoOptions } from '../../state/widgets';
import { openWidget } from '../widgets/launch';
import { font } from '../../theme/palette';
import { IconButton } from '../components';
import { Icon, type IconName } from '../icons';
import { usePalette } from '../theme';
import { headerCall } from './stage';
import { useOpenStage } from './useOpenStage';

/** The most choices Android's alert holds. */
const ALERT_CHOICES = 3;

/** What a video option says in the chooser. */
function optionLabel(option: VideoOption): string {
  switch (option.kind) {
    case 'call':
      return t('call.start_video');
    case 'meeting':
      return t('widgets.join_meeting_named', { name: option.name });
    case 'start':
      return t('mobile.widgets.start_meeting');
  }
}

export function CallButtons({ account, roomId }: { account: AccountId; roomId: string }) {
  const store = useAppStore();
  const router = useRouter();
  const openStage = useOpenStage();
  const palette = usePalette();
  const room = useStore((s) => roomOf(s, account, roomId));
  const meetings = useStore((s) => roomMeetings(s, account, roomId), shallow);
  const current = useStore((s) => s.call.current);
  const shown = headerCall(room, current, account);
  // While Join or Return shows, the video button offers the meetings only.
  const callUnderWay = shown.kind === 'join' || shown.kind === 'return';

  const options = useMemo(
    () => videoOptions(room, meetings, callUnderWay),
    [room, meetings, callUnderWay],
  );

  const call = (video: boolean) => void startCall(store, account, roomId, video);

  const run = (option: VideoOption) => {
    switch (option.kind) {
      case 'call':
        call(true);
        break;
      case 'meeting':
        openWidget(router, account, roomId, { type: 'widget', widgetId: option.widgetId });
        break;
      case 'start':
        openWidget(router, account, roomId, { type: 'meeting', video: true });
        break;
    }
  };

  const choose = () => {
    if (Platform.OS === 'ios') {
      const labels = [...options.map(optionLabel), t('common.cancel')];

      ActionSheetIOS.showActionSheetWithOptions(
        { options: labels, cancelButtonIndex: options.length },
        (index) => {
          const option = options[index];

          if (option) run(option);
        },
      );

      return;
    }

    const shownHere =
      options.length > ALERT_CHOICES ? options.slice(0, ALERT_CHOICES - 1) : options;

    const buttons: AlertButton[] = shownHere.map((option) => ({
      text: optionLabel(option),
      onPress: () => run(option),
    }));

    if (shownHere.length < options.length)
      buttons.push({
        text: t('widgets.title'),
        onPress: () => router.push(routes.widgets(account, roomId)),
      });

    Alert.alert(t('call.start_video'), undefined, buttons, { cancelable: true });
  };

  const [first] = options;

  const video =
    first === undefined ? null : (
      <IconButton
        testID="call-start-video"
        icon="video"
        label={optionLabel(first)}
        hint={options.length > 1 ? t('mobile.widgets.video_more') : undefined}
        color={palette.textMuted}
        onPress={() => run(first)}
        onLongPress={options.length > 1 ? choose : undefined}
      />
    );

  switch (shown.kind) {
    case 'none':
      return video;
    case 'return':
      return (
        <View style={styles.row}>
          <Pill testID="call-return" icon="call" label={t('call.return')} onPress={openStage} />
          {video}
        </View>
      );
    case 'join':
      return (
        <View style={styles.row}>
          <Pill
            testID="call-join"
            icon="call"
            label={t('call.join')}
            hint={t('call.people', { count: shown.count })}
            onPress={() => call(false)}
          />
          {video}
        </View>
      );
    case 'start':
      return (
        <View style={styles.row}>
          <IconButton
            testID="call-start"
            icon="call"
            label={t('call.start')}
            color={palette.textMuted}
            onPress={() => call(false)}
          />
          {video}
        </View>
      );
  }
}

/** A rounded green button with an icon and a label, as the desktop's `pill`. */
function Pill({
  icon,
  label,
  hint,
  onPress,
  testID,
}: {
  icon: IconName;
  label: string;
  hint?: string;
  onPress: () => void;
  testID: string;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      hitSlop={6}
      style={({ pressed }) => [
        styles.pill,
        { backgroundColor: palette.success, opacity: pressed ? 0.8 : 1 },
      ]}
    >
      <Icon name={icon} size={16} color={palette.onAccent} />
      <Text style={[font.bodyStrong, { color: palette.onAccent }]}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: 4 },
  pill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 32,
    paddingHorizontal: 12,
    borderRadius: 16,
  },
});
