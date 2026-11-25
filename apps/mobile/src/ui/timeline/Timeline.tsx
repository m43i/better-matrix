// A timeline on screen, for a room or a thread: its plan (state/plan.ts) drawn by a virtual
// list that starts at the newest message and holds the reading position as older pages arrive
// above it. Scrolling near the top loads the page before; reaching the newest marks it read;
// scrolled up, a button jumps back down. A quoted message, or the `target` a link names,
// scrolls into view and is highlighted; one not loaded yet is looked for a few pages back
// (state/seek.ts).

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
import { unmarkOpened } from '../../state/actions/shell';
import { releaseTimeline, retainTimeline } from '../../state/actions/timeline';
import { clearFocused, setFocused } from '../../state/focus';
import { addNotice } from '../../state/reducer';
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
  type TopRow,
  wantsOlder,
} from '../../state/plan';
import { shallow, useAppStore, useStore } from '../../state/react';
import {
  openSeek,
  quoteHere,
  quoteTapped,
  type Reach,
  seekEnded,
  seekStep,
  sought,
  soughtReach,
} from '../../state/seek';
import {
  account as accountOf,
  room as roomOf,
  shownIgnored,
  shownTimeline,
  timeline as timelineOf,
  unnamedSenders,
} from '../../state/selectors';
import { routes } from '../../routes';
import { font } from '../../theme/palette';
import { Icon } from '../icons';
import type { Jump } from '../links/jumps';
import { beginJump, openElsewhere } from '../links/open';
import { usePalette } from '../theme';
import { useShown } from '../useShown';
import {
  PinnedContext,
  SpoilerContext,
  TimelineContext,
  type TimelineContextValue,
  useSpoilers,
} from './context';
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

/** No one ignored, as one array, so the plan isn't made again for a new empty one. */
const NO_ONE: string[] = [];

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
  /**
   * How far `target` is looked for (`state/seek.ts`): a few pages back, or on to the start of
   * history for a search hit in local history or a message the engine located here.
   */
  targetReach?: Reach;
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
  targetReach = 'near',
}: TimelineProps) {
  const palette = usePalette();
  const store = useAppStore();
  // Whether this screen is still on show when a jump elsewhere is answered.
  const shown = useShown();
  // The spoilers tapped open, by message, place and words, while the timeline is on screen.
  const spoilers = useSpoilers();
  const thread = timelineKey.thread;
  // What people whose ignore is still on its way said draws as nothing (ADR 0011).
  const timeline = useStore((s) => shownTimeline(s, account, timelineKey));

  // The room's pins, once for every row (each row asking would search the rooms per change).
  const pinnedIds = useStore(
    (s) => roomOf(s, account, timelineKey.room_id)?.pinned ?? NO_PINS,
    shallow,
  );

  const pinned = useMemo(() => new Set(pinnedIds), [pinnedIds]);

  // Whom bubbles leave unnamed here, read alone: a room's other changes don't redraw the list.
  const unnamedIds = useStore(
    (s) => unnamedSenders(roomOf(s, account, timelineKey.room_id)),
    shallow,
  );

  const unnamed = useMemo(() => new Set(unnamedIds), [unnamedIds]);

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
  const showRoomChanges = useStore((s) => s.settings.showRoomChanges);
  // What people the user ignores sent never draws, hidden events included, and nor does what
  // someone whose ignore is being undone sent until that is saved (ADR 0011).
  const ignored = useStore((s) => shownIgnored(accountOf(s, account)) ?? NO_ONE, shallow);

  const rows = useMemo(
    () =>
      planTimeline(timeline, {
        thread,
        root,
        marker: shownMarker,
        showHidden,
        showRoomChanges,
        ignored,
      }),
    [timeline, thread, root, shownMarker, showHidden, showRoomChanges, ignored],
  );

  const list = useRef<FlashListRef<Row>>(null);
  // The list starts drawing at the newest; opened at a linked message, it is positioned once it
  // jumped there, gave up looking, or the reader moved on with a quote (state/seek.ts), so the
  // newest isn't marked read on the way. A quote's own search never holds read marking back.
  const [atLatest, setAtLatest] = useState(true);
  const [seek, setSeek] = useState(() => openSeek(target, targetReach));

  if (seek.target !== target || seek.targetReach !== targetReach)
    setSeek(openSeek(target, targetReach));
  const positioned = seek.positioned;
  const [farFromLatest, setFarFromLatest] = useState(false);
  const [highlighted, setHighlighted] = useState<string | null>(null);
  const onScreen = useOnScreen(account, timelineKey);

  // Seeing the room, on opening it or coming back to it, takes away its unread mark.
  useEffect(() => {
    if (onScreen && timelineKey.thread === null) unmarkOpened(store, account, timelineKey.room_id);
  }, [onScreen, store, account, timelineKey]);

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

  // The topmost row on screen, but the loading and failure rows, which come and go: whether the
  // reader is at the start of the history loaded (`wantsOlder`).
  const [top, setTop] = useState<TopRow | null>(null);

  const onViewableItemsChanged = useCallback(
    ({ viewableItems }: { viewableItems: ViewToken<Row>[] }) => {
      if (viewableItems.some((token) => token.item?.kind === 'new'))
        setSeenMarker(sdkMarkerNow.current);

      let first: ViewToken<Row> | null = null;

      for (const token of viewableItems) {
        const kind = token.item?.kind;

        if (kind === undefined || kind === 'loading' || kind === 'historyFailed') continue;

        if (first === null || (token.index ?? Infinity) < (first.index ?? Infinity)) first = token;
      }

      const key = first?.item?.key;
      setTop(key === undefined || first?.index == null ? null : { key, index: first.index });
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

  const rowsRef = useRef(rows);
  useLayoutEffect(() => {
    rowsRef.current = rows;
  }, [rows]);

  const loadMore = useCallback(() => {
    const now = timelineOf(store.getState(), account, timelineKey);

    if (!atStart(now, thread) && now.historyError === null) loadOlder(store, account, timelineKey);
  }, [store, account, timelineKey, thread]);

  // While the reader stays at the start (or nothing draws), the list doesn't say so again: ask
  // for the page before it until one moves the start off screen.
  const pageOlder = wantsOlder(rows, timeline, thread, top);
  useEffect(() => {
    if (pageOlder) loadMore();
  }, [pageOlder, rows, loadMore]);
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

  // The message looked for: the link's target, or one a quote points at that isn't loaded yet,
  // looked for as a link's target is.
  const wanted = sought(seek);
  const quote = seek.quote;
  // The jump the quote's tap began (`beginJump`): through paging and the engine's answer, it
  // goes on only while no later jump began and this screen is on show.
  const quoteJump = useRef<Jump | null>(null);
  // A target the engine located here (a quote's or a link's jump opened this screen at it) may
  // page far back: it goes on as a jump of this screen's own, begun as the screen opens at it,
  // so a jump begun since or leaving the screen stops it. Begun in an effect before the search's
  // own, which runs after it (effects run in order).
  const targetJump = useRef<Jump | null>(null);
  useEffect(() => {
    targetJump.current =
      targetReach === 'located' && target !== null ? beginJump(() => shown.current) : null;
  }, [target, targetReach, shown]);

  // A linked message: scroll to it once loaded, paging back a little to find it (on to the start
  // of history for a hit known to be local, or one the engine says shows here).
  const reach = soughtReach(seek);
  const targetDone = useRef<string | null>(null);
  const targetPages = useRef(0);
  useEffect(() => {
    if (!wanted || targetDone.current === wanted) return;
    const ended = () => setSeek((now) => seekEnded(now, wanted));
    const jump = quote !== null ? quoteJump.current : targetJump.current;

    const step = seekStep({
      quote: quote !== null,
      wanted: jump?.() !== false,
      loaded: rowOf(rows, wanted) >= 0,
      busy: timeline.opening || timeline.loadingOlder || timeline.historyError !== null,
      atStart: reachedStart,
      pages: targetPages.current,
      budget: TARGET_PAGES,
      reach,
    });

    switch (step) {
      case 'stop':
        // A later jump began, or the user left: the search stops, quietly.
        targetDone.current = wanted;
        ended();

        return;
      case 'scroll':
        targetDone.current = wanted;
        // Once the list laid the rows out; the search ends once the jump's scroll was reported.
        requestAnimationFrame(() => {
          scrollTo(wanted, quote !== null);

          if (settleTimer.current) clearTimeout(settleTimer.current);
          settleTimer.current = setTimeout(ended, SETTLE_MS);
        });

        return;
      case 'wait':
        // For the timeline to open, for the page coming, or for Try again after a failure
        // (until then nothing is marked read under the reader).
        return;
      case 'page':
        targetPages.current += 1;
        loadMore();

        return;
      case 'missing':
        // History started without it, though the engine said it shows here: it is gone.
        targetDone.current = wanted;
        ended();
        store.update((s) => addNotice(s, t('mobile.conversation.message_missing'), account));

        return;
      case 'elsewhere':
        // Not found: the list stays where it is. A quoted message may live in another timeline
        // (a reply in a thread, a room message quoted in one): it opens there, as a link's does;
        // one the engine says shows here is looked for further back.
        targetDone.current = wanted;
        ended();

        if (quote !== null && jump !== null)
          void openElsewhere(store, account, timelineKey, wanted, jump, router).then((outcome) => {
            if (outcome.type === 'here') {
              // Looked for as the engine located it: a quoted edit is its original message,
              // scrolled to at once when loaded, else paged for.
              targetDone.current = null;
              targetPages.current = 0;
              setSeek((now) => quoteHere(now, wanted, outcome.eventId));
            } else if (outcome.type === 'missing') {
              store.update((s) => addNotice(s, t('mobile.conversation.message_missing'), account));
            }
          });
    }
  }, [
    store,
    account,
    timelineKey,
    quote,
    reach,
    wanted,
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
      unnamed,
      scrollTo: (eventId) => {
        // The tap is the jump: from now on an earlier one's late answer (a link being looked up,
        // another quote's) opens nothing, and this one's goes on only while this screen is on show.
        const jump = beginJump(() => shown.current);

        if (scrollTo(eventId)) {
          setSeek((now) => quoteTapped(now, null));

          return;
        }

        // Not loaded: look for it a few pages back.
        quoteJump.current = jump;
        targetPages.current = 0;
        targetDone.current = null;
        setSeek((now) => quoteTapped(now, eventId));
      },
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
      shown: () => shown.current,
    }),
    [account, timelineKey, thread, unnamed, scrollTo, shown],
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
        <SpoilerContext.Provider value={spoilers}>
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
                empty ? (
                  <ActivityIndicator style={styles.spinner} color={palette.textMuted} />
                ) : null
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
        </SpoilerContext.Provider>
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
