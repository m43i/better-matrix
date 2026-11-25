// The user's own GIPHY API key: why it's needed and where to get one free. A key kept in the
// secure store is never shown again: the form then says one is saved and offers Replace (an
// empty field, Save and Cancel) and Remove. Either waits for the secure store before the form
// moves on; a failure leaves it as it was, with a notice. The GIF picker shows it until there is a key, or
// with the field open when GIPHY refused the one kept; Preferences opens it to manage the key.

import { useState } from 'react';
import { Linking, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { saveGiphyKey } from '../../state/actions/stickers';
import { useAppStore, useStore } from '../../state/react';
import { GIPHY_KEYS_URL } from '../../state/stickers';
import { font } from '../../theme/palette';
import { Button, TextField } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';

export function GiphyKeyForm({
  onSaved,
  replacing = false,
}: {
  onSaved?: () => void;
  replacing?: boolean;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const kept = useStore((s) => s.giphyKey !== null);
  // Typing a key: a first one, or one replacing the key kept. The field always starts empty.
  const [typing, setTyping] = useState(replacing);
  const [key, setKey] = useState('');
  // A save or removal on its way to the secure store: the form waits for it.
  const [busy, setBusy] = useState(false);
  const editing = !kept || typing;
  const valid = key.trim() !== '' && !busy;

  const save = () => {
    if (!valid) return;
    setBusy(true);
    void saveGiphyKey(store, key).then((saved) => {
      setBusy(false);

      if (!saved) return;
      setKey('');
      setTyping(false);
      onSaved?.();
    });
  };

  const remove = () => {
    setBusy(true);
    void saveGiphyKey(store, null).then(() => setBusy(false));
  };

  return (
    <View style={styles.form} testID="giphy-key-form">
      <View style={styles.heading}>
        <Icon name="key" size={32} color={palette.textMuted} />
        <Text style={[font.header, styles.center, { color: palette.text }]}>
          {t('mobile.stickers.giphy_needed')}
        </Text>
        <Text style={[font.body, styles.center, { color: palette.textMuted }]}>
          {t('mobile.stickers.giphy_explain')}
        </Text>
      </View>
      {editing ? (
        <>
          <Button
            testID="giphy-key-get"
            kind="secondary"
            icon="external"
            title={t('mobile.stickers.giphy_get')}
            onPress={() => void Linking.openURL(GIPHY_KEYS_URL)}
          />
          <TextField
            testID="giphy-key-field"
            label={t('mobile.stickers.giphy_key')}
            value={key}
            onChangeText={setKey}
            autoCapitalize="none"
            autoCorrect={false}
            secureTextEntry
            returnKeyType="done"
            onSubmitEditing={save}
          />
          <Button
            testID="giphy-key-save"
            title={t('mobile.stickers.giphy_save')}
            disabled={!valid}
            onPress={save}
          />
          {kept ? (
            <Button
              testID="giphy-key-cancel"
              kind="plain"
              title={t('mobile.stickers.giphy_cancel')}
              onPress={() => {
                setKey('');
                setTyping(false);
              }}
            />
          ) : null}
        </>
      ) : (
        <>
          <Text
            style={[font.body, styles.center, { color: palette.text }]}
            testID="giphy-key-saved"
          >
            {t('mobile.stickers.giphy_saved')}
          </Text>
          <Button
            testID="giphy-key-replace"
            kind="secondary"
            title={t('mobile.stickers.giphy_replace')}
            onPress={() => setTyping(true)}
          />
          <Button
            testID="giphy-key-remove"
            kind="plain"
            title={t('mobile.stickers.giphy_remove')}
            disabled={busy}
            onPress={remove}
          />
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  form: { gap: 14, paddingHorizontal: 16, paddingVertical: 12 },
  heading: { alignItems: 'center', gap: 8, paddingVertical: 8 },
  center: { textAlign: 'center' },
});
