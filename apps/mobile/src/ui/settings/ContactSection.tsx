// One kind of contact detail on the Profile page: the linked ones, and (where the server lets the
// app change them) adding another, confirmed by a link (email) or a texted code (phone).

import type { AccountId, ContactDetail, ContactKind } from '@better-matrix/matrix-native/types';
import { getLocales } from 'expo-localization';
import { Fragment, useReducer, useState } from 'react';
import { Alert, StyleSheet, Text, View } from 'react-native';

import { t } from '../../i18n';
import { addContact, confirmContactCode, contactLabel, removeContact, requestContactCode } from '../../state/actions/accounts';
import { useAppStore } from '../../state/react';
import { font } from '../../theme/palette';
import { Button, Divider, ListRow, ListSection, TextField } from '../components';
import { usePalette } from '../theme';
import { canRequest, contactForm, EMPTY_CONTACT_FORM } from './contactForm';

/** The country a phone number without `+` is dialled from: the system's region. */
function region(): string {
  return getLocales().find((l) => l.regionCode)?.regionCode ?? '';
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function ContactSection({
  account,
  kind,
  details,
  changeable,
  askPassword,
}: {
  account: AccountId;
  kind: ContactKind;
  /** The linked details, all kinds; `null` while they load. */
  details: ContactDetail[] | null;
  changeable: boolean;
  askPassword: (error: string | null) => Promise<string | null>;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const [form, dispatch] = useReducer(contactForm, EMPTY_CONTACT_FORM);
  const [address, setAddress] = useState('');
  const [code, setCode] = useState('');
  const email = kind === 'Email';
  const id = email ? 'email' : 'phone';
  const mine = details?.filter((d) => d.kind === kind) ?? null;

  const request = () => {
    if (!canRequest(form, address)) return;
    dispatch({ type: 'request' });
    setCode('');
    requestContactCode(store, account, kind, address, region()).then(
      (pending) => dispatch({ type: 'sent', pending }),
      (error: unknown) => dispatch({ type: 'failed', error: messageOf(error) }),
    );
  };

  const confirm = async () => {
    if (form.step.type !== 'sent' || form.step.busy) return;
    const pending = form.step.pending;
    if (!email && !code.trim()) return;
    dispatch({ type: 'confirm' });
    try {
      if (!email) await confirmContactCode(store, account, pending, code);
      const added = await addContact(store, account, pending, { askPassword });
      if (added) {
        dispatch({ type: 'reset' });
        setAddress('');
      } else {
        dispatch({ type: 'idle' });
      }
    } catch (error) {
      dispatch({ type: 'failed', error: messageOf(error) });
    }
  };

  const remove = (contact: ContactDetail) => {
    const label = contactLabel(contact);
    Alert.alert(t('profile.remove', { address: label }), undefined, [
      { text: t('common.cancel'), style: 'cancel' },
      { text: t('common.remove'), style: 'destructive', onPress: () => removeContact(store, account, contact) },
    ]);
  };

  return (
    <ListSection
      title={email ? t('profile.emails') : t('profile.phones')}
      footer={email ? t('profile.emails_hint') : t('profile.phones_hint')}
    >
      {mine === null ? (
        <ListRow title={t('profile.loading')} />
      ) : mine.length === 0 ? (
        <ListRow title={email ? t('profile.no_emails') : t('profile.no_phones')} />
      ) : (
        mine.map((contact, index) => (
          <Fragment key={contact.address}>
            {index > 0 ? <Divider inset={52} /> : null}
            <ListRow
              testID={`contact-${id}-${contact.address}`}
              icon={email ? 'mail' : 'phone'}
              title={contactLabel(contact)}
              chevron={false}
              onPress={changeable ? () => remove(contact) : undefined}
              value={changeable ? t('common.remove') : undefined}
            />
          </Fragment>
        ))
      )}
      {changeable ? (
        <>
          <Divider />
          <View style={styles.form}>
            {form.step.type === 'sent' ? (
              <>
                <Text style={[font.body, { color: palette.text }]}>
                  {email
                    ? t('profile.link_sent', { address: form.step.pending.address })
                    : t('profile.code_sent', { number: form.step.pending.address })}
                </Text>
                {!email ? (
                  <TextField
                    testID={`contact-code-${id}`}
                    value={code}
                    onChangeText={setCode}
                    placeholder={t('profile.code_placeholder')}
                    keyboardType="number-pad"
                    autoComplete="sms-otp"
                    textContentType="oneTimeCode"
                    onSubmitEditing={() => void confirm()}
                  />
                ) : null}
                <View style={styles.buttons}>
                  <Button
                    testID={`contact-cancel-${id}`}
                    title={t('common.cancel')}
                    kind="secondary"
                    onPress={() => dispatch({ type: 'reset' })}
                  />
                  <Button
                    testID={`contact-confirm-${id}`}
                    title={email ? t('profile.link_opened') : t('profile.confirm_code')}
                    busy={form.step.busy}
                    disabled={!email && !code.trim()}
                    onPress={() => void confirm()}
                  />
                </View>
              </>
            ) : (
              <>
                <TextField
                  testID={`contact-new-${id}`}
                  value={address}
                  onChangeText={setAddress}
                  placeholder={email ? t('profile.email_placeholder') : t('profile.phone_placeholder')}
                  keyboardType={email ? 'email-address' : 'phone-pad'}
                  autoComplete={email ? 'email' : 'tel'}
                  autoCapitalize="none"
                  autoCorrect={false}
                  editable={form.step.type === 'entering'}
                  onSubmitEditing={request}
                />
                <Button
                  testID={`contact-request-${id}`}
                  title={
                    form.step.type === 'requesting'
                      ? t('profile.sending')
                      : email
                        ? t('profile.send_link')
                        : t('profile.send_code')
                  }
                  kind="secondary"
                  busy={form.step.type === 'requesting'}
                  disabled={!canRequest(form, address)}
                  onPress={request}
                />
              </>
            )}
            {form.error ? <Text style={[font.body, { color: palette.danger }]}>{form.error}</Text> : null}
          </View>
        </>
      ) : null}
    </ListSection>
  );
}

const styles = StyleSheet.create({
  form: { padding: 16, gap: 12 },
  buttons: { flexDirection: 'row', gap: 8, justifyContent: 'flex-end' },
});
