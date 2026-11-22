// A timeline on screen, for a room or a thread: its plan (state/plan.ts) drawn by a virtual
// list that starts at the newest message and holds the reading position as older pages arrive
// above it. Scrolling near the top loads the page before; reaching the newest marks it read;
// scrolled up, a button jumps back down. A quoted message, or the `target` a link names,
// scrolls into view and is highlighted.

import type { AccountId, Message, TimelineKey } from '@better-matrix/matrix-native/types';
import {
  FlashList,
  type FlashListRef,
  type ListRenderItem,
  type ViewToken,
} from '@shopify/flash-list';
import * as Haptics from 'expo-haptics';
import { router, useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useReducedMotion } from 'react-native-reanimated';
import {
  ActivityIndicator,
  AppState,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { t } from '../../i18n';
import { loadOlder, markRead, mayMarkFullyRead, visitRoom } from '../../state/actions/core';
import { releaseTimeline, retainTimeline } from '../../state/actions/timeline';
import { clearFocused, setFocused } from '../../state/focus';
import {
  MARKER_FADE_MS,
  MARKER_FADE_REDUCED_MS,
  markerAcknowledged,
  markerFaded,
  NO_MARKER,
  nextMarker,
  sdkBoundary,
} from '../../state/newMarker';
import {
  atStart,
  type Marked,
  newestSent,
  planTimeline,
  type Row,
  rowOf,
  shouldMarkRead,
} from '../../state/plan';
import { shallow, useAppStore, useStore } from '../../state/react';
import { room as roomOf, timeline as timelineOf } from '../../state/selectors';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { PinnedContext, TimelineContext, type TimelineContextValue } from './context';
import { MessageRow } from './MessageRow';
import {
  DateDivider,
  HistoryFailedRow,
  LoadingRow,
  NewMarker,
  RepliesDivider,
  HiddenEventLine,
  RoomEventLine,
  StartBlock,
} from './Rows';

const NO_PINS: string[] = [];

/** Within this far of the newest message counts as at it. */
const AT_LATEST = 48;

/** How long a message scrolled to stays highlighted. */
const HIGHLIGHT_MS = 2500;

/** How many pages to load back looking for a linked message before giving up. */
const TARGET_PAGES = 10;

/** After jumping to a linked message: long enough for the list to report where it stands. */
const SETTLE_MS = 250;

export interface TimelineProps {
  account: AccountId;
  timelineKey: TimelineKey;
  /** A thread's root as the room's timeline has it, until the thread's own arrives. */
  root?: Message | null;
  /** A message to scroll to and highlight once it is loaded (a link to it). */
  target?: string | null;
  /** A hit known to be in local history: keep paging until it is found or history ends. */
  localTarget?: boolean;
}

/** Whether the app is in front and this screen is the one on top. */
function useOnScreen(account: AccountId, timelineKey: TimelineKey): boolean {
  const [focused, setFocusedState] = useState(false);
  const [active, setActive] = useState(AppState.currentState === 'active');
  useFocusEffect(
    useCallback(() => {
      setFocused(account, timelineKey);
      setFocusedState(true);

      return () => {
        clearFocused(account, timelineKey);
        setFocusedState(false);
      };
    }, [account, timelineKey]),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) =>
      setActive(state === 'active'),
    );

    return () => subscription.remove();
  }, []);

  return focused && active;
}

export function Timeline({
  account,
  timelineKey,
  root = null,
  target = null,
  localTarget = false,
}: TimelineProps) {
  const palette = usePalette();
  const store = useAppStore();
  const thread = timelineKey.thread;
  const timeline = useStore((s) => timelineOf(s, account, timelineKey));

  // The room's pins, once for every row (each row asking would search the rooms per change).
  const pinnedIds = useStore(
    (s) => roomOf(s, account, timelineKey.room_id)?.pinned ?? NO_PINS,
    shallow,
  );

  const pinned = useMemo(() => new Set(pinnedIds), [pinnedIds]);
  // The "New" divider stays where it first showed and fades once read past (state/newMarker.ts),
  // derived during render so it is there on the first frame.
  const sdkMarker = useMemo(() => sdkBoundary(timeline.entries), [timeline.entries]);
  const [marker, setMarker] = useState(() => nextMarker(NO_MARKER, sdkMarker));
  const nextState = nextMarker(marker, sdkMarker);

  if (nextState !== marker) setMarker(nextState);
  const reducedMotion = useReducedMotion();
  const fading = marker.shown?.fading ?? false;
  useEffect(() => {
    if (!fading) return;

    const timer = setTimeout(
      () => setMarker(markerFaded),
      reducedMotion ? MARKER_FADE_REDUCED_MS : MARKER_FADE_MS,
    );

    return () => clearTimeout(timer);
  }, [fading, reducedMotion]);
  const shownMarker = marker.shown;

  const showHidden = useStore((s) => s.settings.showHiddenEvents);

  const rows = useMemo(
    () => planTimeline(timeline, { thread, root, marker: shownMarker, showHidden }),
    [timeline, thread, root, shownMarker, showHidden],
  );

  const list = useRef<FlashListRef<Row>>(null);
  // The list starts drawing at the newest; opened at a linked message, it is positioned once it
  // jumped there (or gave up looking), so the newest isn't marked read on the way.
  const [atLatest, setAtLatest] = useState(true);
  const [positionedAt, setPositionedAt] = useState<string | null>(null);
  const positioned = target === null || positionedAt === target;
  const [farFromLatest, setFarFromLatest] = useState(false);
  const [highlighted, setHighlighted] = useState<string | null>(null);
  const onScreen = useOnScreen(account, timelineKey);

  // The engine follows the timeline while a screen shows it (sheets over it act on it too).
  useEffect(() => {
    if (timelineKey.thread === null) visitRoom(store, account, timelineKey.room_id);
    retainTimeline(store, account, timelineKey);

    return () => releaseTimeline(store, account, timelineKey);
  }, [store, account, timelineKey]);

  // Where the unread messages begin, once the "New" divider was on screen: until then the
  // cross-device fully-read marker stays put (`markerAcknowledged`).
  const [seenMarker, setSeenMarker] = useState<string | null>(null);
  const sdkMarkerNow = useRef(sdkMarker);
  useLayoutEffect(() => {
    sdkMarkerNow.current = sdkMarker;
  }, [sdkMarker]);

  const onViewableItemsChanged = useCallback(
    ({ viewableItems }: { viewableItems: ViewToken<Row>[] }) => {
      if (viewableItems.some((token) => token.item?.kind === 'new'))
        setSeenMarker(sdkMarkerNow.current);
    },
    [],
  );

  // Marking read: the newest message on screen while the user looks at it.
  const lastMarked = useRef<Marked | null>(null);
  const newest = newestSent(timeline.entries);
  const readBoundary = timeline.readBoundary;
  useEffect(() => {
    const fullyRead =
      mayMarkFullyRead(store, account, timelineKey) && markerAcknowledged(sdkMarker, seenMarker);

    if (
      !shouldMarkRead({
        focused: onScreen,
        positioned,
        atLatest,
        newest,
        fullyRead,
        lastMarked: lastMarked.current,
      })
    )
      return;

    if (newest === null) return;
    lastMarked.current = { eventId: newest, fullyRead };
    markRead(store, account, timelineKey, newest, fullyRead);
  }, [
    store,
    account,
    timelineKey,
    onScreen,
    positioned,
    atLatest,
    newest,
    readBoundary,
    sdkMarker,
    seenMarker,
  ]);

  const reachedStart = atStart(timeline, thread);

  const loadMore = useCallback(() => {
    const now = timelineOf(store.getState(), account, timelineKey);

    if (!atStart(now, thread) && now.historyError === null) loadOlder(store, account, timelineKey);
  }, [store, account, timelineKey, thread]);

  const rowsRef = useRef(rows);
  useLayoutEffect(() => {
    rowsRef.current = rows;
  }, [rows]);
  const highlightTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const scrollTo = useCallback((eventId: string, animated = true): boolean => {
    const index = rowOf(rowsRef.current, eventId);

    if (index < 0) return false;
    void list.current?.scrollToIndex({ index, viewPosition: 0.5, animated });
    setHighlighted(eventId);

    if (highlightTimer.current) clearTimeout(highlightTimer.current);
    highlightTimer.current = setTimeout(() => setHighlighted(null), HIGHLIGHT_MS);

    return true;
  }, []);

  const settleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (highlightTimer.current) clearTimeout(highlightTimer.current);

      if (settleTimer.current) clearTimeout(settleTimer.current);
    },
    [],
  );

  // A linked message: scroll to it once loaded, paging back a little to find it.
  const targetDone = useRef<string | null>(null);
  const targetPages = useRef(0);
  useEffect(() => {
    if (!target || targetDone.current === target) return;

    if (rowOf(rows, target) >= 0) {
      targetDone.current = target;
      // Once the list laid the rows out; it is positioned once the jump's scroll was reported.
      requestAnimationFrame(() => {
        scrollTo(target, false);
        settleTimer.current = setTimeout(() => setPositionedAt(target), SETTLE_MS);
      });
    } else if (timeline.opening || timeline.loadingOlder || timeline.historyError !== null) {
      // Wait: for the timeline to open, for the page coming, or for Try again after a failure
      // (until then nothing is marked read under the reader).
    } else if (!reachedStart && (localTarget || targetPages.current < TARGET_PAGES)) {
      targetPages.current += 1;
      loadMore();
    } else {
      // Not found: the list stays at the newest.
      targetDone.current = target;
      setPositionedAt(target);
    }
  }, [
    target,
    localTarget,
    rows,
    reachedStart,
    timeline.opening,
    timeline.loadingOlder,
    timeline.historyError,
    scrollTo,
    loadMore,
  ]);

  const context = useMemo<TimelineContextValue>(
    () => ({
      account,
      timelineKey,
      thread,
      scrollTo: (eventId) => void scrollTo(eventId),
      openActions: (message) => {
        void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
        router.push(
          routes.messageActions(
            account,
            timelineKey.room_id,
            message.event_id,
            thread ?? undefined,
          ),
        );
      },
    }),
    [account, timelineKey, thread, scrollTo],
  );

  const onScroll = useCallback((event: NativeSyntheticEvent<NativeScrollEvent>) => {
    const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent;
    const fromEnd = contentSize.height - layoutMeasurement.height - contentOffset.y;
    setAtLatest(fromEnd < AT_LATEST);
    setFarFromLatest(fromEnd > layoutMeasurement.height);
  }, []);

  const renderItem: ListRenderItem<Row> = useCallback(({ item, extraData }) => {
    switch (item.kind) {
      case 'message':
        return (
          <MessageRow
            message={item.message}
            continues={item.continues}
            latestCall={item.latestCall}
            highlighted={extraData === item.message.event_id}
          />
        );
      case 'events':
        return <RoomEventLine events={item.events} />;
      case 'date':
        return <DateDivider at={item.at} />;
      case 'new':
        return <NewMarker fading={item.fading} />;
      case 'start':
        return <StartBlock />;
      case 'loading':
        return <LoadingRow />;
      case 'historyFailed':
        return <HistoryFailedRow error={item.error} />;
      case 'replies':
        return <RepliesDivider count={item.count} />;
      case 'hidden':
        return <HiddenEventLine event={item.event} />;
    }
  }, []);

  const empty = !reachedStart && timeline.historyError === null;

  return (
    <TimelineContext.Provider value={context}>
      <PinnedContext.Provider value={pinned}>
        <View style={styles.frame}>
          <FlashList
            ref={list}
            data={rows}
            renderItem={renderItem}
            keyExtractor={keyOf}
            getItemType={typeOf}
            extraData={highlighted}
            onScroll={onScroll}
            onViewableItemsChanged={onViewableItemsChanged}
            scrollEventThrottle={32}
            onStartReached={loadMore}
            onStartReachedThreshold={1}
            maintainVisibleContentPosition={{
              autoscrollToBottomThreshold: 0.2,
              animateAutoScrollToBottom: true,
              startRenderingFromBottom: true,
            }}
            keyboardDismissMode="interactive"
            keyboardShouldPersistTaps="handled"
            contentContainerStyle={styles.content}
            ListEmptyComponent={
              empty ? <ActivityIndicator style={styles.spinner} color={palette.textMuted} /> : null
            }
          />
          {farFromLatest ? (
            <Pressable
              onPress={() => list.current?.scrollToEnd({ animated: true })}
              accessibilityRole="button"
              accessibilityLabel={t('mobile.conversation.jump_latest')}
              style={[
                styles.jump,
                { backgroundColor: palette.surface, borderColor: palette.divider },
              ]}
            >
              <Icon name="arrowDown" size={16} color={palette.accent} />
              <Text style={[font.section, { color: palette.accent }]}>
                {t('mobile.conversation.jump_latest')}
              </Text>
            </Pressable>
          ) : null}
        </View>
      </PinnedContext.Provider>
    </TimelineContext.Provider>
  );
}

function keyOf(row: Row): string {
  return row.key;
}

/** Rows of a kind recycle into each other; messages by their content. */
function typeOf(row: Row): string {
  return row.kind === 'message' ? `message-${row.message.content.type}` : row.kind;
}

const styles = StyleSheet.create({
  frame: { flex: 1 },
  content: { paddingTop: 8, paddingBottom: 8 },
  spinner: { marginTop: 48 },
  jump: {
    position: 'absolute',
    right: 16,
    bottom: 12,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 14,
    height: 36,
    borderRadius: 18,
    borderWidth: StyleSheet.hairlineWidth,
    shadowColor: '#000',
    shadowOpacity: 0.15,
    shadowRadius: 8,
    shadowOffset: { width: 0, height: 2 },
    elevation: 4,
  },
});
