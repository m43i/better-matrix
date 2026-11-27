// Shared building blocks, the mobile counterpart of the desktop's `ui/components.rs`. Screens
// compose these and read colours from the palette; platform conventions (ripple on Android,
// highlight on iOS, grouped lists) live here so screens don't branch on the platform.

import { type ReactNode } from 'react';
import {
  ActivityIndicator,
  Platform,
  Pressable,
  type PressableProps,
  StyleSheet,
  Text,
  TextInput,
  type TextInputProps,
  View,
  type ViewStyle,
} from 'react-native';

import { font } from '../theme/palette';
import { Icon, type IconName } from './icons';
import { usePalette } from './theme';
import { Toggle } from './Toggle';

/** Android draws Material's forms where iOS draws its own (list sections, choices). */
const ANDROID = Platform.OS === 'android';

/** A pressable surface with the platform's press feedback. */
export function Touchable({
  style,
  children,
  ...props
}: PressableProps & { style?: ViewStyle; children: ReactNode }) {
  const palette = usePalette();

  return (
    <Pressable
      android_ripple={{ color: palette.hover, foreground: true }}
      style={({ pressed }) => [
        style,
        Platform.OS === 'ios' && pressed ? { backgroundColor: palette.hover } : null,
      ]}
      {...props}
    >
      {children}
    </Pressable>
  );
}

export type ButtonKind = 'primary' | 'secondary' | 'danger' | 'plain';

export function Button({
  title,
  onPress,
  kind = 'primary',
  busy = false,
  disabled = false,
  icon,
  small = false,
  testID,
}: {
  title: string;
  onPress: () => void;
  kind?: ButtonKind;
  busy?: boolean;
  disabled?: boolean;
  icon?: IconName;
  /** A compact button, for inside a list row. */
  small?: boolean;
  testID?: string;
}) {
  const palette = usePalette();

  const bg =
    kind === 'primary'
      ? palette.accent
      : kind === 'danger'
        ? palette.danger
        : kind === 'secondary'
          ? palette.surface
          : 'transparent';

  const fg =
    kind === 'primary' || kind === 'danger'
      ? palette.onAccent
      : kind === 'plain'
        ? palette.accent
        : palette.text;

  const inactive = disabled || busy;

  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityState={{ disabled: inactive, busy }}
      disabled={inactive}
      onPress={onPress}
      android_ripple={{ color: '#ffffff33' }}
      style={({ pressed }) => [
        styles.button,
        small ? styles.buttonSmall : null,
        {
          backgroundColor: bg,
          borderColor: kind === 'secondary' ? palette.divider : 'transparent',
          opacity: inactive ? 0.55 : pressed && Platform.OS === 'ios' ? 0.8 : 1,
        },
      ]}
    >
      {busy ? (
        <ActivityIndicator color={fg} />
      ) : (
        <View style={styles.buttonContent}>
          {icon ? <Icon name={icon} size={18} color={fg} /> : null}
          <Text style={[font.bodyStrong, { color: fg }]}>{title}</Text>
        </View>
      )}
    </Pressable>
  );
}

/** A labelled text field with an optional hint or error below it. */
export function TextField({
  label,
  hint,
  error,
  ...input
}: TextInputProps & { label?: string; hint?: string; error?: string | null }) {
  const palette = usePalette();

  return (
    <View style={styles.field}>
      {label ? <Text style={[font.section, { color: palette.text }]}>{label}</Text> : null}
      <TextInput
        placeholderTextColor={palette.textMuted}
        selectionColor={palette.accent}
        {...input}
        style={[
          styles.input,
          font.body,
          {
            color: palette.text,
            backgroundColor: palette.surface,
            borderColor: error ? palette.danger : palette.divider,
          },
          input.style,
        ]}
      />
      {error ? (
        <Text style={[font.meta, { color: palette.danger }]}>{error}</Text>
      ) : hint ? (
        <Text style={[font.meta, { color: palette.textMuted }]}>{hint}</Text>
      ) : null}
    </View>
  );
}

/** A count of unread messages; red when they mention the user. */
export function Badge({ count, mention = false }: { count: number; mention?: boolean }) {
  const palette = usePalette();

  if (count <= 0) return null;

  return (
    <View style={[styles.badge, { backgroundColor: mention ? palette.mention : palette.accent }]}>
      <Text style={[font.badge, { color: palette.onAccent }]} allowFontScaling={false}>
        {count > 99 ? '99+' : count}
      </Text>
    </View>
  );
}

/**
 * A group of rows: iOS's inset grouped list (an upper-case caption over a rounded card), or
 * Material's list section on Android (a caption in the accent colour over flat, full-width rows).
 */
export function ListSection({
  title,
  footer,
  children,
}: {
  title?: string;
  footer?: string;
  children: ReactNode;
}) {
  const palette = usePalette();

  return (
    <View style={styles.section}>
      {title ? (
        <Text
          style={[
            styles.sectionTitle,
            font.section,
            { color: ANDROID ? palette.accent : palette.textMuted },
          ]}
        >
          {ANDROID ? title : title.toUpperCase()}
        </Text>
      ) : null}
      <View
        style={
          ANDROID
            ? null
            : [
                styles.sectionBody,
                { backgroundColor: palette.surface, borderColor: palette.divider },
              ]
        }
      >
        {children}
      </View>
      {footer ? (
        <Text style={[styles.sectionFooter, font.meta, { color: palette.textMuted }]}>
          {footer}
        </Text>
      ) : null}
    </View>
  );
}

/**
 * One row of a list: an icon or avatar, a title and subtitle, and a value, switch or chevron. The
 * chevron is iOS's disclosure indicator; Material lists have none, so Android never draws it. A
 * row of a single choice (`selected` given) shows iOS's checkmark when chosen, or Material's
 * radio button on Android; a row of a multiple choice (`checked` given) the checkmark, or
 * Material's checkbox.
 */
export function ListRow({
  title,
  titleContent,
  subtitleContent,
  subtitle,
  icon,
  leading,
  value,
  onPress,
  onLongPress,
  chevron = onPress !== undefined && Platform.OS === 'ios',
  toggle,
  danger = false,
  selected,
  checked,
  titleLines = 1,
  testID,
}: {
  title: string;
  titleContent?: ReactNode;
  subtitle?: string;
  subtitleContent?: ReactNode;
  icon?: IconName;
  leading?: ReactNode;
  value?: string;
  onPress?: () => void;
  onLongPress?: () => void;
  chevron?: boolean;
  toggle?: { value: boolean; onChange: (value: boolean) => void };
  danger?: boolean;
  selected?: boolean;
  checked?: boolean;
  /** How many lines the title may take before it is cut short; 0 for as many as it needs. */
  titleLines?: number;
  testID?: string;
}) {
  const palette = usePalette();
  const color = danger ? palette.danger : palette.text;

  const body = (
    <View style={styles.row}>
      {leading ??
        (icon ? (
          <Icon name={icon} size={22} color={danger ? palette.danger : palette.textMuted} />
        ) : null)}
      <View style={styles.rowText}>
        <Text style={[font.body, { color }]} numberOfLines={titleLines}>
          {titleContent ?? title}
        </Text>
        {subtitle || subtitleContent ? (
          <Text style={[font.meta, { color: palette.textMuted }]} numberOfLines={2}>
            {subtitleContent ?? subtitle}
          </Text>
        ) : null}
      </View>
      {value ? <Text style={[font.body, { color: palette.textMuted }]}>{value}</Text> : null}
      {ANDROID && selected !== undefined ? (
        <Icon
          name={selected ? 'choiceOn' : 'choice'}
          size={22}
          color={selected ? palette.accent : palette.textMuted}
        />
      ) : selected ? (
        <Icon name="check" size={20} color={palette.accent} />
      ) : null}
      {ANDROID && checked !== undefined ? (
        <Icon
          name={checked ? 'checkboxOn' : 'checkbox'}
          size={22}
          color={checked ? palette.accent : palette.textMuted}
        />
      ) : checked ? (
        <Icon name="check" size={20} color={palette.accent} />
      ) : null}
      {toggle ? <Toggle value={toggle.value} onChange={toggle.onChange} label={title} /> : null}
      {chevron && !toggle && Platform.OS === 'ios' ? (
        <Icon name="chevronRight" size={16} color={palette.textMuted} />
      ) : null}
    </View>
  );

  if (!onPress) return <View testID={testID}>{body}</View>;

  return (
    <Touchable
      testID={testID}
      onPress={onPress}
      onLongPress={onLongPress}
      accessibilityRole={checked === undefined ? 'button' : 'checkbox'}
      accessibilityState={
        checked !== undefined ? { checked } : selected === undefined ? undefined : { selected }
      }
    >
      {body}
    </Touchable>
  );
}

export function Divider({ inset = 0 }: { inset?: number }) {
  const palette = usePalette();

  return (
    <View
      style={{
        height: StyleSheet.hairlineWidth,
        marginLeft: inset,
        backgroundColor: palette.divider,
      }}
    />
  );
}

/** What a screen shows when it has nothing to list, or while it loads. */
export function EmptyState({
  icon,
  title,
  body,
  loading = false,
}: {
  icon?: IconName;
  title: string;
  body?: string;
  loading?: boolean;
}) {
  const palette = usePalette();

  return (
    <View style={styles.empty}>
      {loading ? (
        <ActivityIndicator color={palette.textMuted} />
      ) : icon ? (
        <Icon name={icon} size={40} color={palette.textMuted} />
      ) : null}
      <Text style={[font.header, { color: palette.text, textAlign: 'center' }]}>{title}</Text>
      {body ? (
        <Text style={[font.body, { color: palette.textMuted, textAlign: 'center' }]}>{body}</Text>
      ) : null}
    </View>
  );
}

/** A round icon button, as headers and the call controls use. */
export function IconButton({
  icon,
  onPress,
  onLongPress,
  label,
  hint,
  color,
  background,
  size = 40,
  testID,
  expanded,
}: {
  icon: IconName;
  onPress: () => void;
  /** More choices, as a long press offers them (an action sheet). */
  onLongPress?: () => void;
  label: string;
  /** What else it does, for screen readers (a long press's choices). */
  hint?: string;
  color?: string;
  background?: string;
  size?: number;
  testID?: string;
  expanded?: boolean;
}) {
  const palette = usePalette();

  return (
    <Pressable
      testID={testID}
      onPress={onPress}
      onLongPress={onLongPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={expanded === undefined ? undefined : { expanded }}
      // Screen readers reach the long press's choices as the button's own action.
      accessibilityActions={onLongPress ? [{ name: 'longpress' }] : undefined}
      onAccessibilityAction={(event) => {
        if (event.nativeEvent.actionName === 'longpress') onLongPress?.();
      }}
      hitSlop={8}
      android_ripple={{ color: palette.hover, borderless: true, radius: size / 2 }}
      style={({ pressed }) => ({
        width: size,
        height: size,
        borderRadius: size / 2,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: background ?? 'transparent',
        opacity: pressed && Platform.OS === 'ios' ? 0.6 : 1,
      })}
    >
      <Icon name={icon} size={size * 0.55} color={color ?? palette.text} />
    </Pressable>
  );
}

const styles = StyleSheet.create({
  button: {
    minHeight: 48,
    borderRadius: 10,
    paddingHorizontal: 18,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
  },
  buttonSmall: { minHeight: 34, paddingHorizontal: 12, borderRadius: 8 },
  buttonContent: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  field: { gap: 6 },
  input: {
    borderWidth: 1,
    borderRadius: 10,
    paddingHorizontal: 14,
    paddingVertical: Platform.OS === 'ios' ? 13 : 10,
  },
  badge: {
    minWidth: 20,
    height: 20,
    borderRadius: 10,
    paddingHorizontal: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  section: { marginTop: 22, marginHorizontal: ANDROID ? 0 : 16 },
  sectionTitle: { marginLeft: ANDROID ? 18 : 16, marginBottom: ANDROID ? 2 : 6 },
  sectionBody: { borderRadius: 12, overflow: 'hidden', borderWidth: StyleSheet.hairlineWidth },
  sectionFooter: { marginHorizontal: ANDROID ? 18 : 16, marginTop: 6 },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    paddingHorizontal: 16,
    paddingVertical: 12,
    minHeight: 52,
  },
  rowText: { flex: 1, gap: 2 },
  empty: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 10, padding: 32 },
});
