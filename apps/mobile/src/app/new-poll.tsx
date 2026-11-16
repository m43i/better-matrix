// Creating a poll, as a modal: the question, two to twenty options (added and removed), and
// whether results show while people vote. Create sends it to the room or thread it was opened
// from and closes; it is offered only once the poll is ready, as on the desktop.

import { Stack, useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { t } from '../i18n';
import { keyOf, sendPoll } from '../state/actions/composer';
import {
  addAnswer,
  canAddAnswer,
  canRemoveAnswer,
  newPollForm,
  pollReady,
  removeAnswer,
  setAnswer,
} from '../state/pollForm';
import { useAppStore } from '../state/react';
import { font } from '../theme/palette';
import { Button, IconButton, ListRow, ListSection, TextField } from '../ui/components';
import { usePalette } from '../ui/theme';

function HeaderButton({
  title,
  onPress,
  disabled = false,
  strong = false,
  testID,
}: {
  title: string;
  onPress: () => void;
  disabled?: boolean;
  strong?: boolean;
  testID: string;
}) {
  const palette = usePalette();

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityState={{ disabled }}
      hitSlop={8}
      testID={testID}
    >
      <Text
        style={[
          strong ? font.bodyStrong : font.body,
          { color: palette.accent, opacity: disabled ? 0.4 : 1 },
        ]}
      >
        {title}
      </Text>
    </Pressable>
  );
}

export default function NewPollScreen() {
  const { account, roomId, thread } = useLocalSearchParams<{
    account: string;
    roomId: string;
    thread?: string;
  }>();

  const store = useAppStore();
  const router = useRouter();
  const palette = usePalette();
  const [form, setForm] = useState(newPollForm);
  const ready = pollReady(form);

  const create = () => {
    if (sendPoll(store, account, keyOf(roomId, thread), form)) router.back();
  };

  return (
    <>
      <Stack.Screen
        options={{
          title: t('poll.title'),
          headerLeft: () => (
            <HeaderButton
              title={t('common.cancel')}
              onPress={() => router.back()}
              testID="poll-cancel"
            />
          ),
          headerRight: () => (
            <HeaderButton
              title={t('poll.create')}
              onPress={create}
              disabled={!ready}
              strong
              testID="poll-create"
            />
          ),
        }}
      />
      <KeyboardAwareScrollView
        style={{ backgroundColor: palette.bg }}
        contentContainerStyle={styles.content}
        keyboardShouldPersistTaps="handled"
        bottomOffset={24}
      >
        <View style={styles.block}>
          <TextField
            testID="poll-question"
            label={t('poll.question')}
            placeholder={t('poll.question_placeholder')}
            value={form.question}
            onChangeText={(question) => setForm((f) => ({ ...f, question }))}
            autoFocus
            returnKeyType="next"
          />
        </View>
        <View style={styles.block}>
          <Text style={[font.section, { color: palette.text }]}>{t('poll.answers')}</Text>
          {form.answers.map((answer, index) => (
            // Answers have no identity but their place; a removed one shifts the rest up.
            <View key={index} style={styles.answer}>
              <View style={styles.answerField}>
                <TextField
                  testID={`poll-answer-${index}`}
                  placeholder={t('poll.answer_placeholder', { number: index + 1 })}
                  accessibilityLabel={t('poll.answer_placeholder', { number: index + 1 })}
                  value={answer}
                  onChangeText={(text) => setForm((f) => setAnswer(f, index, text))}
                  returnKeyType={index === form.answers.length - 1 ? 'done' : 'next'}
                />
              </View>
              {canRemoveAnswer(form) ? (
                <IconButton
                  icon="minusCircle"
                  label={t('poll.remove_answer')}
                  onPress={() => setForm((f) => removeAnswer(f, index))}
                  color={palette.danger}
                  size={36}
                  testID={`poll-remove-${index}`}
                />
              ) : null}
            </View>
          ))}
          {canAddAnswer(form) ? (
            <View style={styles.add}>
              <Button
                title={t('poll.add_answer')}
                icon="plus"
                kind="plain"
                onPress={() => setForm(addAnswer)}
                testID="poll-add"
              />
            </View>
          ) : null}
        </View>
        <ListSection
          footer={form.disclosed ? t('poll.disclosed_hint') : t('poll.undisclosed_hint')}
        >
          <ListRow
            testID="poll-disclosed"
            icon="eye"
            title={t('poll.disclosed')}
            toggle={{
              value: form.disclosed,
              onChange: (disclosed) => setForm((f) => ({ ...f, disclosed })),
            }}
          />
        </ListSection>
      </KeyboardAwareScrollView>
    </>
  );
}

const styles = StyleSheet.create({
  content: { paddingTop: 8, paddingBottom: 32 },
  block: { marginTop: 18, marginHorizontal: 16, gap: 8 },
  answer: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  answerField: { flex: 1 },
  add: { alignSelf: 'flex-start' },
});
