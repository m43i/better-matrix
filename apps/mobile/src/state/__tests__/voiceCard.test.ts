import {
  WAVE_HEIGHT,
  WAVE_MAX_BARS,
  WAVE_MIN_BAR,
  WAVE_MIN_BARS,
  WAVEFORM_MAX,
  barHeights,
  meterLevel,
  nextSpeed,
  shareAt,
  voiceDetails,
  waveBars,
  waveformFromLevels,
} from '../voiceCard';

describe('waveBars', () => {
  it('should fit as many bars as the width takes, within the least and the most', () => {
    expect([waveBars(178), waveBars(20), waveBars(2000)]).toEqual([
      36,
      WAVE_MIN_BARS,
      WAVE_MAX_BARS,
    ]);
  });
});

describe('barHeights', () => {
  it('should scale the waveform to its loudest bar', () => {
    expect(barHeights([0, 256, 512], 3)).toEqual([
      WAVE_MIN_BAR,
      (WAVE_MIN_BAR + WAVE_HEIGHT) / 2,
      WAVE_HEIGHT,
    ]);
  });

  it('should draw flat bars for an unknown waveform', () => {
    expect(barHeights([], 4)).toEqual([WAVE_MIN_BAR, WAVE_MIN_BAR, WAVE_MIN_BAR, WAVE_MIN_BAR]);
  });

  it('should keep each stretch’s peak in a long waveform', () => {
    expect(barHeights([0, 1024, 0, 0], 2)[0]).toBe(WAVE_HEIGHT);
  });
});

describe('shareAt', () => {
  it('should keep a press past either end of the bars at that end', () => {
    expect([shareAt(-4, 100), shareAt(25, 100), shareAt(140, 100)]).toEqual([0, 0.25, 1]);
  });
});

describe('nextSpeed', () => {
  it('should step through the speeds and back to the start', () => {
    expect([nextSpeed(1), nextSpeed(1.5), nextSpeed(2)]).toEqual([1.5, 2, 1]);
  });
});

describe('meterLevel', () => {
  it('should read silence as nothing and full scale as all', () => {
    expect([meterLevel(undefined), meterLevel(-90), meterLevel(-30), meterLevel(0)]).toEqual([
      0, 0, 0.5, 1,
    ]);
  });
});

describe('waveformFromLevels', () => {
  it('should show loud stretches higher than quiet ones', () => {
    const [quiet, loud] = waveformFromLevels([-45, -45, -6, -6], 2);
    expect(quiet!).toBeLessThan(loud!);
  });

  it('should keep amplitudes within the waveform’s range', () => {
    expect(waveformFromLevels([-160, 0, 3], 3)).toEqual([0, WAVEFORM_MAX, WAVEFORM_MAX]);
  });

  it('should send no waveform for a recording that heard nothing', () => {
    expect(waveformFromLevels([])).toEqual([]);
  });
});

describe('voiceDetails', () => {
  it('should read a voice memo’s length in seconds', () => {
    expect(voiceDetails({ voice: true, duration_ms: 4200, waveform: [1] })).toEqual({
      voice: true,
      seconds: 4.2,
      waveform: [1],
    });
  });

  it('should take an audio file without details as no voice memo of unknown length', () => {
    expect(voiceDetails(null)).toEqual({ voice: false, seconds: null, waveform: [] });
  });
});
