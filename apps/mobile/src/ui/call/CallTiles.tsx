// The call plan on screen (the desktop's `ui::call::stage` viewport): every planned tile at its
// rectangle, the participant strip's divider, swiping between pages, and the paging row. Both
// the full stage (`app/call.tsx`) and the compact stage over the chat (`CompactCall`) draw with
// it. Only planned tiles mount, and only a mounted `CallVideoView` watches its video, so hidden
// pages and a hidden self-view convert no frames (ADR 0010).

import type { CallInfo } from '@better-matrix/matrix-native/types';
import { type ReactNode, useLayoutEffect, useMemo, useState } from 'react';
import {
  type LayoutChangeEvent,
  PanResponder,
  type PanResponderInstance,
  Pressable,
  StyleSheet,
  Text,
  View,
} from 'react-native';

import { CallVideoView } from '../../../modules/call-media';
import { t } from '../../i18n';
import { pageCall, pinCallTile, setCallStrip, toggleCallFit } from '../../state/actions/calls';
import {
  type CallPlan,
  type CallTile,
  type PlacedTile,
  sameTile,
  tileId,
} from '../../state/callLayout';
import {
  type CallView,
  callVideo,
  clampStrip,
  planCall,
  showsFullVideo,
  withStrip,
} from '../../state/callView';
import type { Store } from '../../state/store';
import { useAppStore, useStore } from '../../state/react';
import { account as accountOf } from '../../state/selectors';
import { font } from '../../theme/palette';
import { Avatar, avatarRadius } from '../Avatar';
import { IconButton } from '../components';
import { Icon } from '../icons';
import { usePalette } from '../theme';
import { describeTile, type Tile } from './stage';
import type { HeardCall } from './useCallAudio';

/** The size of a face on a voice call's stage, at most. */
const FACE = 96;

/** How far a swipe goes before it turns the page. */
const SWIPE = 48;

/** The plan for a viewport the caller measures with `onLayout`; `preview` shows a strip being dragged. */
export function useCallPlan(
  info: CallInfo,
  view: CallView,
  speaker: string | null,
  compact: boolean,
) {
  const [size, setSize] = useState<[number, number]>([0, 0]);
  const [dragged, preview] = useState<number | null>(null);

  const planned = useMemo(
    () =>
      planCall(info, dragged === null ? view : withStrip(view, dragged), size, speaker, compact),
    [info, view, dragged, size, speaker, compact],
  );

  const onLayout = (event: LayoutChangeEvent) => {
    const { width, height } = event.nativeEvent.layout;
    setSize((old) =>
      Math.abs(old[0] - width) < 0.5 && Math.abs(old[1] - height) < 0.5 ? old : [width, height],
    );
  };

  return { planned, size, onLayout, preview };
}

export function CallTiles({
  info,
  view,
  planned,
  size,
  heard,
  compact,
  onLayout,
  preview,
  onTilePress,
}: {
  info: CallInfo;
  view: CallView;
  planned: CallPlan;
  size: [number, number];
  heard: HeardCall;
  compact: boolean;
  onLayout: (event: LayoutChangeEvent) => void;
  preview: (strip: number | null) => void;
  /** Instead of pinning the tile (the compact stage opens the full one). */
  onTilePress?: () => void;
}) {
  const store = useAppStore();
  const video = callVideo(info);
  const [swipe] = useState(() => new CallSwipe(store, planned));
  useLayoutEffect(() => swipe.update(planned), [swipe, planned]);

  const pin = view.layout.type === 'Focus' ? view.layout.pin : null;

  return (
    <View
      testID="call-viewport"
      style={styles.viewport}
      onLayout={onLayout}
      {...swipe.responder.panHandlers}
    >
      {planned.tiles.map((placed) => {
        const tile = describeTile(info, placed.key);
        const press = onTilePress ?? (video ? () => pinCallTile(store, placed.key) : undefined);

        return (
          <PlacedView key={tileId(placed.key)} placed={placed}>
            {video ? (
              <VideoTile
                info={info}
                view={view}
                tile={tile}
                callTile={placed.key}
                rect={placed.rect}
                inset={placed.inset}
                speaking={heard.speaking.has(tile.speaker) && !tile.screen}
                pinned={pin !== null && sameTile(pin, placed.key)}
                compact={compact}
                onPress={press}
              />
            ) : (
              <VoiceTile
                info={info}
                tile={tile}
                rect={placed.rect}
                speaking={heard.speaking.has(tile.speaker)}
                onPress={onTilePress}
              />
            )}
          </PlacedView>
        );
      })}
      {planned.divider && !compact ? (
        <Divider
          rect={planned.divider}
          beside={planned.stripBeside}
          size={size}
          strip={view.strip}
          preview={preview}
        />
      ) : null}
    </View>
  );
}

function PlacedView({ placed, children }: { placed: PlacedTile; children: ReactNode }) {
  const [x, y, w, h] = placed.rect;

  return (
    <View style={{ position: 'absolute', left: x, top: y, width: w, height: h }}>{children}</View>
  );
}

/** One surface: shared screens show their whole image, cameras Full video or Fill tile. */
function VideoTile({
  info,
  view,
  tile,
  callTile,
  rect,
  inset,
  speaking,
  pinned,
  compact,
  onPress,
}: {
  info: CallInfo;
  view: CallView;
  tile: Tile;
  callTile: CallTile;
  rect: PlacedTile['rect'];
  inset: boolean;
  speaking: boolean;
  pinned: boolean;
  compact: boolean;
  onPress?: () => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const [, , w, h] = rect;
  const full = showsFullVideo(info, view, callTile);
  const video = tile.video;
  // A fit button on a tile too small to see it would cover the face.
  const fit = !tile.screen && video !== null && !compact && !inset && w >= 120 && h >= 80;

  return (
    <Pressable
      testID={`call-${tile.key}`}
      accessibilityRole="button"
      accessibilityLabel={t('call.focus_on', { name: tile.name })}
      accessibilityState={{ selected: pinned }}
      disabled={!onPress}
      onPress={onPress}
      style={[styles.tile, inset ? { borderWidth: 1, borderColor: palette.divider } : null]}
    >
      {video !== null && CallVideoView ? (
        <CallVideoView
          // A new call numbers its videos afresh: a new view for each.
          key={`${info.call}-${video}`}
          video={video}
          contain={full}
          mirror={tile.own}
          style={StyleSheet.absoluteFill}
        />
      ) : (
        <View style={styles.tileFace}>
          <TileAvatar info={info} tile={tile} size={Math.min(Math.max(h * 0.45, 24), 72)} />
        </View>
      )}
      {w >= 72 ? (
        <View style={[styles.tileLabel, { maxWidth: Math.max(w - 20, 0) }]}>
          <Text style={[font.meta, styles.white]} numberOfLines={1}>
            {tile.name}
          </Text>
          {tile.muted ? <Icon name="micOff" size={12} color="#ffffff" /> : null}
        </View>
      ) : null}
      {fit ? (
        <View style={styles.fit}>
          <IconButton
            testID={`call-${tile.key}-fit`}
            icon={full ? 'cropVideo' : 'fullVideo'}
            label={full ? t('call.crop_video') : t('call.full_video')}
            color="#ffffff"
            background="#00000080"
            size={32}
            onPress={() => toggleCallFit(store, callTile)}
          />
        </View>
      ) : null}
      {speaking ? (
        <View pointerEvents="none" style={[styles.tileRing, { borderColor: palette.accent }]} />
      ) : null}
    </Pressable>
  );
}

/** How far the speaking ring sits outside the face: its padding and border. */
const RING_INSET = 6;

/** A voice call's face, with a ring while they speak. */
function VoiceTile({
  info,
  tile,
  rect,
  speaking,
  onPress,
}: {
  info: CallInfo;
  tile: Tile;
  rect: PlacedTile['rect'];
  speaking: boolean;
  onPress?: () => void;
}) {
  const palette = usePalette();
  const [, , w, h] = rect;
  const size = Math.max(Math.min(FACE, w - 16, h - 40), 24);

  return (
    <Pressable
      testID={`call-face-${tile.key}`}
      disabled={!onPress}
      onPress={onPress}
      style={styles.face}
    >
      <View
        style={[
          styles.faceRing,
          // The avatar's own corners, grown by the ring's padding and border so they run parallel.
          { borderRadius: avatarRadius(size) + RING_INSET },
          speaking
            ? {
                borderColor: palette.accent,
                shadowColor: palette.accent,
                shadowOpacity: 0.55,
                elevation: 8,
              }
            : { borderColor: 'transparent', shadowOpacity: 0 },
        ]}
      >
        <TileAvatar info={info} tile={tile} size={size} />
      </View>
      <View style={[styles.faceName, { maxWidth: Math.min(w, 160) }]}>
        <Text style={[font.bodyStrong, { color: palette.text, flexShrink: 1 }]} numberOfLines={1}>
          {tile.name}
        </Text>
        {tile.muted ? <Icon name="micOff" size={14} color={palette.textMuted} /> : null}
      </View>
    </Pressable>
  );
}

export function TileAvatar({ info, tile, size }: { info: CallInfo; tile: Tile; size: number }) {
  const own = useStore((s) => accountOf(s, info.account)?.summary);

  if (tile.person) {
    return (
      <Avatar
        account={info.account}
        id={tile.person.id}
        name={tile.person.name}
        media={tile.person.avatar}
        size={size}
      />
    );
  }

  return (
    <Avatar
      account={info.account}
      id={info.account}
      name={own?.display_name ?? info.account}
      media={own?.avatar}
      size={size}
    />
  );
}

/** The participant strip's edge: drag it (or adjust it with a screen reader) within 18–40%. */
function Divider({
  rect,
  beside,
  size,
  strip,
  preview,
}: {
  rect: [number, number, number, number];
  beside: boolean;
  size: [number, number];
  strip: number;
  preview: (strip: number | null) => void;
}) {
  const store = useAppStore();
  const palette = usePalette();
  const [drag] = useState(() => new StripDrag(store, { beside, size, strip, preview }));
  useLayoutEffect(
    () => drag.update({ beside, size, strip, preview }),
    [drag, beside, size, strip, preview],
  );

  const [x, y, w, h] = rect;

  return (
    <View
      testID="call-divider"
      accessible
      accessibilityRole="adjustable"
      accessibilityLabel={t('call.resize_strip')}
      accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
      onAccessibilityAction={(event) =>
        setCallStrip(store, strip + (event.nativeEvent.actionName === 'increment' ? 0.02 : -0.02))
      }
      hitSlop={10}
      style={[styles.divider, { left: x, top: y, width: w, height: h }]}
      {...drag.responder.panHandlers}
    >
      <View
        style={[
          { backgroundColor: palette.divider, borderRadius: 2 },
          beside ? { width: 3, height: 32 } : { width: 32, height: 3 },
        ]}
      />
    </View>
  );
}

/** A swipe keeps its native responder while reading the latest committed plan on release. */
class CallSwipe {
  readonly responder: PanResponderInstance;

  constructor(
    store: Store,
    private planned: CallPlan,
  ) {
    this.responder = PanResponder.create({
      onMoveShouldSetPanResponder: (_, gesture) =>
        Math.abs(gesture.dx) > 16 && Math.abs(gesture.dx) > Math.abs(gesture.dy) * 1.5,
      onPanResponderRelease: (_, gesture) => {
        if (Math.abs(gesture.dx) >= SWIPE && this.planned.pages > 1)
          pageCall(store, this.planned, gesture.dx < 0);
      },
    });
  }

  update(planned: CallPlan): void {
    this.planned = planned;
  }
}

interface StripPosition {
  beside: boolean;
  size: [number, number];
  strip: number;
  preview: (strip: number | null) => void;
}

/** Native dragging survives the preview renders it causes; updates arrive after each commit. */
class StripDrag {
  readonly responder: PanResponderInstance;

  constructor(
    store: Store,
    private position: StripPosition,
  ) {
    this.responder = PanResponder.create({
      onStartShouldSetPanResponder: () => true,
      onPanResponderTerminationRequest: () => false,
      onPanResponderMove: (_, gesture) =>
        this.position.preview(this.dragged(gesture.dx, gesture.dy)),
      onPanResponderRelease: (_, gesture) => {
        setCallStrip(store, this.dragged(gesture.dx, gesture.dy));
        this.position.preview(null);
      },
      onPanResponderTerminate: () => this.position.preview(null),
    });
  }

  update(position: StripPosition): void {
    this.position = position;
  }

  private dragged(dx: number, dy: number): number {
    const {
      beside,
      size: [width, height],
      strip,
    } = this.position;

    return clampStrip(strip - (beside ? dx : dy) / Math.max(beside ? width : height, 1));
  }
}

/** The pages of a crowded call: previous, where it is, next. */
export function Paging({ planned, compact = false }: { planned: CallPlan; compact?: boolean }) {
  const store = useAppStore();
  const palette = usePalette();

  if (planned.pages <= 1) return compact ? null : <View style={styles.paging} />;
  const size = compact ? 32 : 36;

  return (
    <View testID="call-paging" style={[styles.paging, compact ? styles.pagingCompact : null]}>
      <IconButton
        testID="call-page-previous"
        icon="chevronLeft"
        label={t('call.previous_page')}
        color={planned.page === 0 ? palette.textMuted : palette.text}
        size={size}
        onPress={() => pageCall(store, planned, false)}
      />
      <Text style={[font.meta, { color: palette.textMuted }]}>
        {t('call.page', { page: planned.page + 1, pages: planned.pages })}
      </Text>
      <IconButton
        testID="call-page-next"
        icon="chevronRight"
        label={t('call.next_page')}
        color={planned.page + 1 >= planned.pages ? palette.textMuted : palette.text}
        size={size}
        onPress={() => pageCall(store, planned, true)}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  viewport: { flex: 1, overflow: 'hidden' },
  tile: { flex: 1, borderRadius: 14, overflow: 'hidden', backgroundColor: '#0000008c' },
  tileFace: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  tileLabel: {
    position: 'absolute',
    left: 10,
    bottom: 10,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 26,
    paddingHorizontal: 10,
    borderRadius: 13,
    backgroundColor: '#00000080',
  },
  white: { color: '#ffffff', flexShrink: 1 },
  fit: { position: 'absolute', top: 6, right: 6 },
  tileRing: {
    position: 'absolute',
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
    borderRadius: 14,
    borderWidth: 2.5,
  },
  face: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: 10 },
  faceRing: {
    borderWidth: 3,
    padding: RING_INSET - 3,
    shadowRadius: 22,
    shadowOffset: { width: 0, height: 0 },
  },
  faceName: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  divider: { position: 'absolute', alignItems: 'center', justifyContent: 'center' },
  paging: {
    height: 40,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 12,
  },
  pagingCompact: { height: 36, gap: 4 },
});
