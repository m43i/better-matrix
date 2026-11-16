import type { TimelineEntry } from '@better-matrix/matrix-native/types';

import { entry } from '../../test/fixtures';
import { markerAcknowledged, markerFaded, NO_MARKER, nextMarker, sdkBoundary } from '../newMarker';

const marker: TimelineEntry = { id: 'marker', kind: { type: 'ReadMarker' } };

describe('sdkBoundary', () => {
  it('should be the first message after the read marker', () => {
    expect(sdkBoundary([entry('$1'), marker, entry('$2'), entry('$3')])).toBe('$2');
  });

  it('should be nothing without a read marker', () => {
    expect(sdkBoundary([entry('$1'), entry('$2')])).toBeNull();
  });
});

describe('nextMarker', () => {
  it('should show the divider where the read marker first stands', () => {
    expect(nextMarker(NO_MARKER, '$2')).toEqual({
      shown: { boundary: '$2', fading: false },
      hidden: false,
    });
  });

  it('should keep the divider in place and fade it when receipts move the marker', () => {
    const shown = nextMarker(NO_MARKER, '$2');
    expect(nextMarker(shown, '$9').shown).toEqual({ boundary: '$2', fading: true });
  });

  it('should keep the divider in place and fade it when receipts remove the marker', () => {
    const shown = nextMarker(NO_MARKER, '$2');
    expect(nextMarker(shown, null).shown).toEqual({ boundary: '$2', fading: true });
  });

  it('should leave a divider whose marker stays put as it is', () => {
    const shown = nextMarker(NO_MARKER, '$2');
    expect(nextMarker(shown, '$2')).toBe(shown);
  });

  it('should show no later divider once one has faded', () => {
    const faded = markerFaded(nextMarker(nextMarker(NO_MARKER, '$2'), null));
    expect(nextMarker(faded, '$7').shown).toBeNull();
  });
});

describe('markerAcknowledged', () => {
  it('an unread boundary not yet on screen should hold the fully-read marker', () => {
    expect(markerAcknowledged('$5', null)).toBe(false);
  });

  it('an unread boundary seen on screen should let the fully-read marker follow', () => {
    expect(markerAcknowledged('$5', '$5')).toBe(true);
  });

  it('no unread boundary in loaded history should hold nothing back', () => {
    expect(markerAcknowledged(null, null)).toBe(true);
  });
});
