//! Sound the app plays and records itself, on the devices the user picked (Settings → Voice &
//! video): voice messages and audio files, voice memos, and the ring of an incoming call. Devices
//! are cpal's (PulseAudio's on Linux, which PipeWire serves too, so their names match the ones
//! calls use), found by name, falling back to the system's default. Files decode in pure Rust:
//! Ogg, WebM and MP4 audio (Opus, Vorbis, AAC), MP3, FLAC and WAV.
//!
//! Each stream runs on a thread of its own and stops when its handle drops. Headless runs (tests,
//! `--snap`) never open a device: they use Rune's player and recorder, which fake theirs.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

/// Voice memos are written at this rate, mono: plenty for speech, a quarter of CD's size.
const MEMO_RATE: u32 = 24_000;
/// The level meter's floor: silence reads this, in dBFS.
const FLOOR_DB: f32 = -160.0;
/// The block a stream asks its device for, in seconds. Left to itself, PulseAudio buffers two
/// seconds: a clip would start (and pause) that long after the press, and a meter would lag.
const BLOCK_SECONDS: f32 = 0.02;
/// Amplitudes in a voice memo's waveform: within the 30 to 120 MSC3246 suggests.
pub const WAVEFORM_POINTS: usize = 100;
/// A waveform's quietest bar, in dBFS: room noise and below read as nothing.
const WAVEFORM_FLOOR_DB: f32 = -50.0;

/// The host whose device names calls share: PulseAudio where there is one (Linux), else the
/// system's.
fn host() -> cpal::Host {
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Ok(host) = cpal::host_from_id(cpal::HostId::PulseAudio) {
        return host;
    }
    cpal::default_host()
}

fn output_device(name: Option<&str>) -> Option<cpal::Device> {
    let host = host();
    name.and_then(|name| host.output_devices().ok()?.find(|device| device.to_string() == name))
        .or_else(|| host.default_output_device())
}

fn input_device(name: Option<&str>) -> Option<cpal::Device> {
    let host = host();
    name.and_then(|name| host.input_devices().ok()?.find(|device| device.to_string() == name))
        .or_else(|| host.default_input_device())
}

/// The device's own stream config, asking for [`BLOCK_SECONDS`] blocks where the device takes
/// a size (PulseAudio, CoreAudio); elsewhere, the device's default.
fn low_latency(supported: &cpal::SupportedStreamConfig) -> cpal::StreamConfig {
    let mut config = supported.config();
    let frames = (config.sample_rate as f32 * BLOCK_SECONDS) as cpal::FrameCount;
    if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size()
        && (*min..=*max).contains(&frames)
    {
        config.buffer_size = cpal::BufferSize::Fixed(frames);
    }
    config
}

// ----- decoding ----------------------------------------------------------------------------

/// Decoded audio: interleaved samples from -1 to 1.
#[derive(Debug, Default)]
pub struct Pcm {
    pub rate: u32,
    pub channels: usize,
    pub samples: Vec<f32>,
}

impl Pcm {
    fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    /// Channel `channel` at fractional frame `at`, between the frames around it; past the last
    /// frame, the last, so a segment's end doesn't dip toward silence where the next one joins.
    fn sample(&self, at: f64, channel: usize) -> f32 {
        let channels = self.channels.max(1);
        let channel = channel.min(channels - 1);
        let last = self.frames().saturating_sub(1);
        let first = at.max(0.0).floor() as usize;
        let mix = (at.max(0.0) - first as f64) as f32;
        let get = |frame: usize| self.samples.get(frame.min(last) * channels + channel).copied().unwrap_or(0.0);
        get(first) * (1.0 - mix) + get(first + 1) * mix
    }
}

/// The loudness along interleaved `samples` of `channels` in `points` amplitudes, each from 0 to
/// [`matrix_core::AudioDetails::WAVEFORM_MAX`]: each stretch's RMS on a decibel scale from
/// [`WAVEFORM_FLOOR_DB`], as ears hear it, so quiet speech still shows. Empty for no samples.
pub fn waveform(samples: &[f32], channels: usize, points: usize) -> Vec<u16> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 || points == 0 {
        return Vec::new();
    }
    let max = f32::from(matrix_core::AudioDetails::WAVEFORM_MAX);
    (0..points)
        .map(|i| {
            let (start, end) = (i * frames / points, ((i + 1) * frames / points).max(i * frames / points + 1));
            let stretch = &samples[start * channels..(end * channels).min(samples.len())];
            let rms = (stretch.iter().map(|s| s * s).sum::<f32>() / stretch.len().max(1) as f32).sqrt();
            let db = if rms <= 1e-8 { WAVEFORM_FLOOR_DB } else { 20.0 * rms.log10() };
            let level = ((db - WAVEFORM_FLOOR_DB) / -WAVEFORM_FLOOR_DB).clamp(0.0, 1.0);
            (level * max).round() as u16
        })
        .collect()
}

/// `pcm` played `rate` times as fast at the same pitch, by WSOLA: windows of 30 ms laid half
/// over each other, each taken from near where `rate` puts it, at the offset (within 10 ms)
/// that best continues the window before, so voices neither chirp nor stutter. The search
/// compares about 8 kHz worth of samples, so its cost doesn't grow with the file's rate. The
/// last window is the file's very end; where it goes in the copy (within half a window of the
/// copy's length) is the place its sound best continues the window before, so the copy is the
/// file's length divided by `rate` give or take a few milliseconds, and ends without a dip.
/// Every sample is divided by the window weight laid on it, so overlaps neither swell nor dip.
/// The first window starts and the last ends at full strength, so neither end fades. `None` at
/// 1×, and for a clip of only a few windows, which plays at 1×.
pub fn stretch(pcm: &Pcm, rate: f64) -> Option<Pcm> {
    let channels = pcm.channels.max(1);
    let frames = pcm.frames();
    let window = (pcm.rate as usize * 30 / 1000).max(8) & !1;
    let hop = window / 2;
    let tolerance = pcm.rate as usize / 100;
    let stride = (pcm.rate as usize / 8000).max(1);
    let target = (frames as f64 / rate).round() as usize;
    if (rate - 1.0).abs() < 1e-3 || rate <= 0.0 || frames < 2 * window + 2 * tolerance || target < window + hop {
        return None;
    }
    // A periodic Hann window: halves laid over each other sum to one.
    let hann: Vec<f32> =
        (0..window).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / window as f32).cos()).collect();
    let mono = |frame: usize| (0..channels).map(|c| pcm.samples[frame * channels + c]).sum::<f32>();
    // How alike the sound from `a` and from `b` is over `len` frames, from -1 to 1.
    let alike = |a: usize, b: usize, len: usize| {
        let (mut both, mut first, mut second) = (0.0f32, 0.0f32, 0.0f32);
        for i in (0..len).step_by(stride) {
            let (x, y) = (mono(a + i), mono(b + i));
            both += x * y;
            first += x * x;
            second += y * y;
        }
        both / (first * second).sqrt().max(1e-9)
    };
    let last = frames - window;
    // Where the windows go in the copy: as many as half-window hops need to reach the copy's
    // end, spread evenly, so the last falls exactly where the copy is due to end and its search
    // has a full hop of room either way, whatever the copy's length.
    let count = (target - window).div_ceil(hop).max(1);
    let place = |k: usize| ((k as f64) * (target - window) as f64 / count as f64).round() as usize;
    let mut out: Vec<f32> = Vec::with_capacity((target + window) * channels);
    let mut weights: Vec<f32> = Vec::with_capacity(target + window);
    // Where the window before went in the copy and was taken from in the file.
    let mut previous: Option<(usize, usize)> = None;
    for k in 0..=count {
        let end = k == count;
        let (at, taken) = match previous {
            // The last window, the file's end: placed where it best continues the one before.
            Some((before_at, before_taken)) if end => {
                let ideal = place(count);
                let low = (before_at + hop / 2).max(ideal.saturating_sub(hop / 2));
                let high = (before_at + window - hop / 2).min(ideal + hop / 2).max(low);
                let mut best = (f32::MIN, ideal.clamp(low, high));
                let mut at = low;
                while at <= high {
                    let overlap = before_at + window - at;
                    let score = alike(before_taken + (at - before_at), last, overlap);
                    if score > best.0 {
                        best = (score, at);
                    }
                    at += stride;
                }
                (best.1, last)
            }
            _ if end => (target - window, last),
            None => (0, 0),
            Some((before_at, before_taken)) => {
                let at = place(k);
                let ideal = ((at as f64 * rate) as usize).min(last);
                // The window before, played on by as far as this one comes after it.
                let natural = before_taken + (at - before_at);
                let (low, high) = (ideal.saturating_sub(tolerance), (ideal + tolerance).min(last));
                let mut best = (f32::MIN, ideal);
                let mut offset = low;
                while natural <= last && offset <= high {
                    let score: f32 =
                        (0..window).step_by(stride).map(|i| mono(natural + i) * mono(offset + i)).sum::<f32>();
                    if score > best.0 {
                        best = (score, offset);
                    }
                    offset += stride;
                }
                (at, best.1)
            }
        };
        out.resize(out.len().max((at + window) * channels), 0.0);
        weights.resize(weights.len().max(at + window), 0.0);
        for (i, hann) in hann.iter().enumerate() {
            let weight = if (k == 0 && i < hop) || (end && i >= hop) { 1.0 } else { *hann };
            weights[at + i] += weight;
            for c in 0..channels {
                out[(at + i) * channels + c] += pcm.samples[(taken + i) * channels + c] * weight;
            }
        }
        if end {
            out.truncate((at + window) * channels);
            weights.truncate(at + window);
            break;
        }
        previous = Some((at, taken));
    }
    for (frame, weight) in out.chunks_mut(channels).zip(&weights) {
        if *weight > 1e-6 {
            frame.iter_mut().for_each(|sample| *sample /= weight);
        }
    }
    Some(Pcm { rate: pcm.rate, channels: pcm.channels, samples: out })
}

/// Decodes the file's first audio track.
pub fn decode(path: &Path) -> Result<Pcm, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{CODEC_TYPE_NULL, CODEC_TYPE_OPUS, DecoderOptions};
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let fail = |e: Error| format!("can't decode this audio: {e}");
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let options = FormatOptions { enable_gapless: true, ..Default::default() };
    let mut format = symphonia::default::get_probe()
        .format(&hint, stream, &options, &MetadataOptions::default())
        .map_err(fail)?
        .format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("no audio track")?;
    let (id, params) = (track.id, track.codec_params.clone());
    let mut pcm = Pcm { rate: params.sample_rate.unwrap_or(48_000), channels: 0, samples: Vec::new() };
    // Opus has no decoder in symphonia: opuscule decodes its packets (RFC 7845).
    let mut opus = if params.codec == CODEC_TYPE_OPUS {
        Some(Opus::new(params.extra_data.as_deref().unwrap_or_default())?)
    } else {
        None
    };
    let mut decoder = match opus {
        Some(_) => None,
        None => Some(symphonia::default::get_codecs().make(&params, &DecoderOptions::default()).map_err(fail)?),
    };
    let mut buffer: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(Error::ResetRequired) => break,
            Err(e) => return Err(fail(e)),
        };
        if packet.track_id() != id {
            continue;
        }
        if let Some(opus) = opus.as_mut() {
            pcm.rate = 48_000;
            pcm.channels = opus.channels;
            opus.decode(&packet.data, &mut pcm.samples)?;
            continue;
        }
        let Some(decoder) = decoder.as_mut() else { break };
        match decoder.decode(&packet) {
            Ok(audio) => {
                let spec = *audio.spec();
                pcm.rate = spec.rate;
                pcm.channels = spec.channels.count();
                let needed = audio.capacity() * spec.channels.count();
                if buffer.as_ref().is_none_or(|b| b.capacity() < needed) {
                    buffer = Some(SampleBuffer::new(audio.capacity() as u64, spec));
                }
                if let Some(buffer) = buffer.as_mut() {
                    buffer.copy_interleaved_ref(audio);
                    pcm.samples.extend_from_slice(buffer.samples());
                }
            }
            // A damaged packet is skipped, as a player would.
            Err(Error::DecodeError(_)) => {}
            Err(e) => return Err(fail(e)),
        }
    }
    if pcm.channels == 0 {
        return Err("the audio is empty".into());
    }
    Ok(pcm)
}

struct Opus {
    decoder: opuscule::Decoder,
    channels: usize,
    /// Decoded frames still to drop for the pre-skip.
    skip: usize,
    gain: f32,
    buffer: Vec<f32>,
}

impl Opus {
    /// From the `OpusHead` identification header (RFC 7845 §5.1).
    fn new(head: &[u8]) -> Result<Self, String> {
        if head.len() < 19 || &head[..8] != b"OpusHead" {
            return Err("Opus audio without its header".into());
        }
        let channels = usize::from(head[9]);
        if head[18] != 0 || !(1..=2).contains(&channels) {
            return Err(format!("Opus with {channels} channels isn't supported"));
        }
        let skip = usize::from(u16::from_le_bytes([head[10], head[11]]));
        let gain_db = f32::from(i16::from_le_bytes([head[16], head[17]])) / 256.0;
        let layout = if channels == 1 { opuscule::Channels::Mono } else { opuscule::Channels::Stereo };
        Ok(Self {
            decoder: opuscule::Decoder::new(opuscule::SampleRate::Hz48000, layout),
            channels,
            skip,
            gain: 10f32.powf(gain_db / 20.0),
            // 120 ms, the longest packet.
            buffer: vec![0.0; 5760 * channels],
        })
    }

    fn decode(&mut self, packet: &[u8], out: &mut Vec<f32>) -> Result<(), String> {
        let frames = match self.decoder.decode(Some(packet), &mut self.buffer, false) {
            Ok(frames) => frames,
            Err(opuscule::Error::InvalidPacket) => return Ok(()),
            Err(e) => return Err(format!("Opus: {e}")),
        };
        let start = self.skip.min(frames);
        self.skip -= start;
        let gain = self.gain;
        out.extend(
            self.buffer[start * self.channels..frames * self.channels].iter().map(|s| (s * gain).clamp(-1.0, 1.0)),
        );
        Ok(())
    }
}

// ----- output ------------------------------------------------------------------------------

/// Starts a stream on `device` playing what `fill` writes (interleaved f32 at the device's rate
/// and channels), until the stream drops; the receiver hears when the device goes for good. The
/// stream stays on the calling thread.
fn open_output(
    device: &cpal::Device,
    fill: impl FnMut(&mut [f32], u32, usize) + Send + 'static,
) -> Result<(cpal::Stream, Receiver<String>), String> {
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let (format, config) = (supported.sample_format(), low_latency(&supported));
    let (rate, channels) = (config.sample_rate, usize::from(config.channels));
    let (lost, gone) = channel();
    let stream = match format {
        SampleFormat::F32 => output_stream::<f32>(device, config, (rate, channels), fill, lost),
        SampleFormat::I16 => output_stream::<i16>(device, config, (rate, channels), fill, lost),
        SampleFormat::U16 => output_stream::<u16>(device, config, (rate, channels), fill, lost),
        SampleFormat::I32 => output_stream::<i32>(device, config, (rate, channels), fill, lost),
        other => Err(format!("the speaker's format {other} isn't supported")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, gone))
}

/// Plays what `fill` writes on `device` until `stop` disconnects. Runs on the calling thread.
fn play_on(
    device: &cpal::Device,
    stop: &Receiver<()>,
    fill: impl FnMut(&mut [f32], u32, usize) + Send + 'static,
) -> Result<(), String> {
    let (_stream, gone) = open_output(device, fill)?;
    // Plays until the handle drops its sender, or the speaker goes.
    loop {
        match stop.recv_timeout(SPEAKER_POLL) {
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(error) = gone.try_recv() {
                    return Err(error);
                }
            }
            _ => return Ok(()),
        }
    }
}

/// How often a thread with a stream open checks that its speaker is still there.
const SPEAKER_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// Whether a stream error means the device is gone for good (unplugged, the sound server gone),
/// rather than a hiccup.
fn device_lost(error: &cpal::Error) -> bool {
    use cpal::ErrorKind;
    matches!(error.kind(), ErrorKind::DeviceNotAvailable | ErrorKind::HostUnavailable | ErrorKind::StreamInvalidated)
}

/// An output stream that `fill`s each block; `lost` hears when the device goes for good
/// (unplugged, the sound server gone), other trouble is only logged.
fn output_stream<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    (rate, channels): (u32, usize),
    mut fill: impl FnMut(&mut [f32], u32, usize) + Send + 'static,
    lost: Sender<String>,
) -> Result<cpal::Stream, String> {
    let mut scratch: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                scratch.resize(out.len(), 0.0);
                scratch.iter_mut().for_each(|s| *s = 0.0);
                fill(&mut scratch, rate, channels);
                for (out, sample) in out.iter_mut().zip(&scratch) {
                    *out = T::from_sample(*sample);
                }
            },
            move |error: cpal::Error| {
                if device_lost(&error) {
                    let _ = lost.send(error.to_string());
                } else {
                    eprintln!("better-matrix: audio output: {error}");
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

/// A file playing (or paused) on a speaker.
#[derive(Debug)]
struct Playback {
    /// The decoded file: let go after the clip has been paused a while
    /// ([`PlayerTiming::release`]), and decoded again to play.
    pcm: Option<Arc<Pcm>>,
    /// The file's frames and rate, kept when `pcm` is let go.
    length: Option<(usize, u32)>,
    /// `pcm` stretched, a segment at a time, to play at a rate other than 1× at its own pitch;
    /// a segment not stretched yet plays at 1×.
    stretched: Option<Stretched>,
    /// Where it is, in the file's frames.
    position: f64,
    /// Where to start once decoded, as a share of the length.
    seek_to: Option<f64>,
    playing: bool,
    /// How fast it plays: 1 is as recorded.
    rate: f64,
    /// The loudness along the file, once decoded.
    waveform: Vec<u16>,
    /// What the last frame was read from.
    source: Option<Source>,
    /// The source before, faded out over [`FADE_SECONDS`] after it changed within a segment
    /// (its stretched copy arriving, or another speed), with the frames of the fade left.
    fade: Option<(Source, u32)>,
    /// The stretches of the file started so far: the latest is [`Stretched::job`].
    jobs: u64,
    /// The player is gone: a stretch still under way stops.
    closed: bool,
}

/// How long playback crossfades when what it reads from changes mid-segment, in seconds.
const FADE_SECONDS: f64 = 0.01;

/// What playback reads: the file, or a stretched segment of it.
#[derive(Clone, Debug)]
struct Source {
    pcm: Arc<Pcm>,
    /// The file's frame the source starts at, and its frames per frame of the file.
    start: f64,
    scale: f64,
    /// The segment it is for, and what it is (the file, or which stretched copy).
    key: (usize, usize),
}

impl Source {
    /// Channel `channel` at frame `position` of the file.
    fn read(&self, position: f64, channel: usize) -> f32 {
        self.pcm.sample((position - self.start) * self.scale, channel)
    }
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            pcm: None,
            length: None,
            stretched: None,
            position: 0.0,
            seek_to: None,
            playing: false,
            rate: 1.0,
            waveform: Vec::new(),
            source: None,
            fade: None,
            jobs: 0,
            closed: false,
        }
    }
}

/// Seconds of the file a stretched segment covers: segments are stretched one at a time, from
/// the one playing, so a new speed or a seek waits for one segment rather than the whole file.
const SEGMENT_SECONDS: u32 = 4;

/// A segment of the file at another speed.
#[derive(Clone, Debug)]
enum Segment {
    Pending,
    /// Too short to stretch: it plays at 1×.
    Plain,
    Stretched(Arc<Pcm>),
}

/// The segment of `count`, each `size` frames (the last taking the rest), holding frame
/// `position` of the file.
fn segment_of(size: usize, count: usize, position: f64) -> usize {
    ((position.max(0.0) as usize) / size.max(1)).min(count.max(1) - 1)
}

/// The segments of a file of `pcm`'s length: their size in frames, and how many.
fn segment_layout(pcm: &Pcm) -> (usize, usize) {
    let size = (SEGMENT_SECONDS * pcm.rate.max(1)) as usize;
    (size, (pcm.frames() / size).max(1))
}

/// The file at another speed, in segments of [`SEGMENT_SECONDS`] (the last takes the rest).
/// Each is stretched alone, starting and ending at full strength on the file's own samples, so
/// they join without a seam.
#[derive(Debug)]
struct Stretched {
    /// Which stretch of the file this is: a worker for an earlier one stops.
    job: u64,
    rate: f64,
    /// Frames of the file a segment covers.
    size: usize,
    frames: usize,
    segments: Vec<Segment>,
}

impl Stretched {
    fn new(job: u64, rate: f64, pcm: &Pcm) -> Self {
        let (size, count) = segment_layout(pcm);
        Self { job, rate, size, frames: pcm.frames(), segments: vec![Segment::Pending; count] }
    }

    /// The segment holding frame `position` of the file.
    fn index(&self, position: f64) -> usize {
        segment_of(self.size, self.segments.len(), position)
    }

    /// The file's frames segment `index` covers.
    fn bounds(&self, index: usize) -> std::ops::Range<usize> {
        let start = index * self.size;
        let end = if index + 1 == self.segments.len() { self.frames } else { start + self.size };
        start..end
    }

    /// The segment to stretch next: the first pending one at or after `from`, else the first.
    fn next(&self, from: usize) -> Option<usize> {
        let pending = |index: &usize| matches!(self.segments[*index], Segment::Pending);
        (from..self.segments.len()).find(pending).or_else(|| (0..from).find(pending))
    }
}

impl Playback {
    /// The file's frames and rate, once decoded.
    fn length(&self) -> Option<(usize, u32)> {
        self.pcm.as_ref().map(|pcm| (pcm.frames(), pcm.rate)).or(self.length)
    }

    /// Takes the decoded file: to where it was to start, if it was moved before decoding, and
    /// stretched to the speed it plays at. Returns the stretch job to start ([`stretch_for`]).
    fn install(&mut self, pcm: Arc<Pcm>) -> Option<(u64, Arc<Pcm>)> {
        if let Some(share) = self.seek_to.take() {
            self.position = share.clamp(0.0, 1.0) * pcm.frames() as f64;
        }
        self.length = Some((pcm.frames(), pcm.rate));
        self.pcm = Some(pcm);
        self.begin_stretch()
    }

    /// Lets the decoded file and its stretched copies go while paused, keeping its length,
    /// place and waveform: it is decoded again to play.
    fn release(&mut self) {
        if self.playing {
            return;
        }
        self.length = self.length();
        self.pcm = None;
        self.stretched = None;
        self.source = None;
        self.fade = None;
    }

    /// Stretch `job`, while it is the latest and the player is still there.
    fn stretch_job(&mut self, job: u64) -> Option<&mut Stretched> {
        let closed = self.closed;
        self.stretched.as_mut().filter(|stretched| stretched.job == job && !closed)
    }

    /// Starts stretching the file to the speed it plays at, unless that is 1× or the file isn't
    /// decoded yet: the job for [`stretch_for`].
    fn begin_stretch(&mut self) -> Option<(u64, Arc<Pcm>)> {
        let pcm = self.pcm.clone().filter(|_| self.rate != 1.0);
        self.stretched = pcm.as_deref().map(|pcm| {
            self.jobs += 1;
            Stretched::new(self.jobs, self.rate, pcm)
        });
        pcm.map(|pcm| (self.jobs, pcm))
    }

    /// The next block for a speaker at `rate` with `channels`: the file from where it is, until
    /// its end, where it stops and rewinds. Nothing while paused or before it is decoded. At
    /// another speed, each stretched segment is read at its own rate so the pitch holds, a place
    /// in the file being a place in the segment scaled by their lengths; a segment not stretched
    /// yet (or too short to stretch) plays at 1×. When what it reads changes within a segment,
    /// the two crossfade, so the switch doesn't pop.
    fn render(&mut self, out: &mut [f32], rate: u32, channels: usize) {
        let Some(pcm) = self.pcm.clone() else { return };
        if !self.playing {
            return;
        }
        let native = f64::from(pcm.rate) / f64::from(rate.max(1));
        let fade_frames = (FADE_SECONDS * f64::from(rate)).max(1.0) as u32;
        for frame in out.chunks_mut(channels.max(1)) {
            if self.position >= pcm.frames() as f64 {
                self.playing = false;
                self.position = 0.0;
                self.source = None;
                self.fade = None;
                break;
            }
            self.follow_source(&pcm, fade_frames);
            let Some(source) = self.source.as_ref() else { break };
            // A fade ends at a segment's join: the source before has nothing past it, and every
            // source there is on the file's own samples.
            if self.fade.as_ref().is_some_and(|(before, _)| before.key.0 != source.key.0) {
                self.fade = None;
            }
            for (channel, out) in frame.iter_mut().enumerate() {
                let sample = source.read(self.position, channel);
                *out = match &self.fade {
                    Some((before, left)) => {
                        let gone = 1.0 - *left as f32 / fade_frames as f32;
                        before.read(self.position, channel) * (1.0 - gone) + sample * gone
                    }
                    None => sample,
                };
            }
            let step = native / source.scale;
            self.fade = self.fade.take().and_then(|(before, left)| (left > 1).then(|| (before, left - 1)));
            self.position += step;
        }
    }

    /// Points [`Playback::source`] at what the frame at the position is read from now; when that
    /// changed within a segment, the source before fades out over `fade_frames`.
    fn follow_source(&mut self, pcm: &Arc<Pcm>, fade_frames: u32) {
        let (size, count) = segment_layout(pcm);
        let index = segment_of(size, count, self.position);
        let copy = self.stretched.as_ref().filter(|stretched| stretched.rate == self.rate).and_then(|stretched| {
            match &stretched.segments[index] {
                Segment::Stretched(copy) => Some((stretched.bounds(index), copy)),
                Segment::Pending | Segment::Plain => None,
            }
        });
        let key = (index, copy.as_ref().map_or(0, |(_, copy)| Arc::as_ptr(copy) as usize));
        if self.source.as_ref().is_some_and(|source| source.key == key) {
            return;
        }
        let source = match copy {
            Some((bounds, copy)) => Source {
                pcm: Arc::clone(copy),
                start: bounds.start as f64,
                scale: (copy.frames() as f64 / bounds.len().max(1) as f64).max(1e-9),
                key,
            },
            None => Source { pcm: Arc::clone(pcm), start: 0.0, scale: 1.0, key },
        };
        // Crossing into the next segment joins on the file's own samples: no fade there.
        if let Some(before) = self.source.replace(source).filter(|before| before.key.0 == index) {
            self.fade = Some((before, fade_frames));
        }
    }
}

/// Stretches `pcm` for stretch `job` on a worker, a segment at a time, each time the pending
/// one at or after where the clip is now. It stops once every segment is done, or once the job
/// is no longer the latest (another speed) or the player is gone.
fn stretch_for(state: &Arc<Mutex<Playback>>, pcm: Arc<Pcm>, job: u64) -> Option<std::thread::JoinHandle<()>> {
    let state = Arc::clone(state);
    std::thread::Builder::new()
        .name("audio-stretch".into())
        .spawn(move || {
            loop {
                let (index, bounds, rate) = {
                    let Ok(mut state) = state.lock() else { return };
                    let position = state.position;
                    let Some(stretched) = state.stretch_job(job) else { return };
                    let Some(index) = stretched.next(stretched.index(position)) else { return };
                    (index, stretched.bounds(index), stretched.rate)
                };
                let channels = pcm.channels.max(1);
                let part = Pcm {
                    rate: pcm.rate,
                    channels: pcm.channels,
                    samples: pcm.samples[bounds.start * channels..bounds.end * channels].to_vec(),
                };
                let segment = stretch(&part, rate).map_or(Segment::Plain, |copy| Segment::Stretched(Arc::new(copy)));
                let Ok(mut state) = state.lock() else { return };
                let Some(stretched) = state.stretch_job(job) else { return };
                stretched.segments[index] = segment;
            }
        })
        .ok()
}

/// When a player opens its speaker and lets go of what it holds.
#[derive(Clone, Copy, Debug)]
struct PlayerTiming {
    /// How often, while the speaker is open, the player checks it is still to play.
    poll: std::time::Duration,
    /// How long the speaker stays open after the clip pauses or ends: the last block drains,
    /// and a quick pause and play go on without opening it again.
    linger: std::time::Duration,
    /// How long a paused clip keeps its decoded file before letting it go.
    release: std::time::Duration,
}

impl PlayerTiming {
    const APP: Self = Self {
        poll: SPEAKER_POLL,
        linger: std::time::Duration::from_millis(500),
        release: std::time::Duration::from_secs(60),
    };
}

fn is_playing(state: &Mutex<Playback>) -> bool {
    state.lock().is_ok_and(|state| state.playing)
}

/// The player's thread once its file is decoded: the speaker is open (`open`) only while the
/// clip plays, so a paused or finished clip costs nothing, and after [`PlayerTiming::release`]
/// paused the decoded file goes too, `load`ed again to play. `wake` hears each play and
/// disconnects when the player goes, which ends the thread.
fn serve<S>(
    state: &Mutex<Playback>,
    wake: &Receiver<()>,
    timing: PlayerTiming,
    mut open: impl FnMut() -> Result<(S, Receiver<String>), String>,
    mut load: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    use std::sync::mpsc::RecvTimeoutError;
    loop {
        let mut released = false;
        while !is_playing(state) {
            let woke = if released {
                wake.recv().map_err(|_| RecvTimeoutError::Disconnected)
            } else {
                wake.recv_timeout(timing.release)
            };
            match woke {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => {
                    if let Ok(mut state) = state.lock() {
                        state.release();
                    }
                    released = true;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
        if state.lock().is_ok_and(|state| state.pcm.is_none()) {
            load()?;
        }
        let (speaker, gone) = open()?;
        let mut quiet_since: Option<std::time::Instant> = None;
        loop {
            if let Err(RecvTimeoutError::Disconnected) = wake.recv_timeout(timing.poll) {
                return Ok(());
            }
            if let Ok(error) = gone.try_recv() {
                return Err(error);
            }
            if is_playing(state) {
                quiet_since = None;
            } else if quiet_since.get_or_insert_with(std::time::Instant::now).elapsed() >= timing.linger {
                break;
            }
        }
        drop(speaker);
    }
}

#[derive(Debug)]
pub struct NativePlayer {
    state: Arc<Mutex<Playback>>,
    /// Wakes the player's thread to play; dropped, the thread ends and its speaker closes.
    wake: Sender<()>,
}

impl Drop for NativePlayer {
    /// A stretch still under way for it stops (its speaker stops as the wake sender drops).
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
    }
}

impl NativePlayer {
    /// Opens `path` for playing on `device` (the default when `None`); it starts paused.
    /// `decoded` runs (on the player's thread) once the file is decoded, so a paused clip's card
    /// can show its length and waveform; `failed` if the file can't be read or played.
    pub fn open(
        path: PathBuf,
        device: Option<String>,
        decoded: impl FnOnce() + Send + 'static,
        failed: impl FnOnce(String) + Send + 'static,
    ) -> Self {
        let state = Arc::new(Mutex::new(Playback::default()));
        let (wake, woken) = channel();
        let shared = Arc::clone(&state);
        let _ = std::thread::Builder::new().name("audio-player".into()).spawn(move || {
            let pcm = match decode(&path) {
                Ok(pcm) => Arc::new(pcm),
                Err(error) => return failed(error),
            };
            // Read before taking the lock, which the speaker's callback and the view wait on.
            let points = waveform(&pcm.samples, pcm.channels, WAVEFORM_POINTS);
            let job = shared.lock().ok().and_then(|mut state| {
                state.waveform = points;
                state.install(Arc::clone(&pcm))
            });
            drop(pcm);
            if let Some((job, pcm)) = job {
                stretch_for(&shared, pcm, job);
            }
            decoded();
            let Some(device) = output_device(device.as_deref()) else {
                return failed("there is no speaker".to_owned());
            };
            let open = || {
                let shared = Arc::clone(&shared);
                open_output(&device, move |out: &mut [f32], rate: u32, channels: usize| {
                    if let Ok(mut state) = shared.lock() {
                        state.render(out, rate, channels);
                    }
                })
            };
            let load = || {
                let pcm = Arc::new(decode(&path)?);
                let job = shared.lock().ok().and_then(|mut state| state.install(pcm));
                if let Some((job, pcm)) = job {
                    stretch_for(&shared, pcm, job);
                }
                Ok(())
            };
            if let Err(error) = serve(&shared, &woken, PlayerTiming::APP, open, load) {
                failed(error);
            }
        });
        Self { state, wake }
    }

    /// A player for `pcm`, already decoded and paused, on no speaker: for tests.
    #[cfg(test)]
    pub fn decoded(pcm: Pcm) -> Self {
        let points = waveform(&pcm.samples, pcm.channels, WAVEFORM_POINTS);
        let state = Playback { pcm: Some(Arc::new(pcm)), waveform: points, ..Playback::default() };
        Self { state: Arc::new(Mutex::new(state)), wake: channel().0 }
    }

    /// Plays from where it is; from the start when it is at the end, or is to start there once
    /// decoded.
    pub fn play(&self) {
        if let Ok(mut state) = self.state.lock() {
            let end = state.length().map_or(f64::MAX, |(frames, _)| frames as f64);
            if state.position >= end {
                state.position = 0.0;
            }
            if state.seek_to.is_some_and(|share| share >= 1.0) {
                state.seek_to = None;
            }
            state.playing = true;
        }
        let _ = self.wake.send(());
    }

    /// How far along it is, from 0 to 1: before it is decoded, where it is to start.
    pub fn progress(&self) -> f64 {
        self.state.lock().map_or(0.0, |state| match state.length() {
            Some((frames, _)) => (state.position / frames.max(1) as f64).clamp(0.0, 1.0),
            None => state.seek_to.unwrap_or(0.0).clamp(0.0, 1.0),
        })
    }

    /// Moves to `share` of the way through (0 the start, 1 the end); before the file is
    /// decoded, it starts there.
    pub fn seek(&self, share: f64) {
        let Ok(mut state) = self.state.lock() else { return };
        match state.length() {
            Some((frames, _)) => state.position = share.clamp(0.0, 1.0) * frames as f64,
            None => state.seek_to = Some(share),
        }
    }

    /// Plays `rate` times as fast, at the same pitch. It is stretched on a worker, a segment at
    /// a time from where it is; a segment not ready yet plays at 1×.
    pub fn set_rate(&self, rate: f64) {
        let job = {
            let Ok(mut state) = self.state.lock() else { return };
            if state.rate == rate {
                return;
            }
            state.rate = rate;
            state.begin_stretch()
        };
        if let Some((job, pcm)) = job {
            stretch_for(&self.state, pcm, job);
        }
    }

    /// The loudness along the file, once decoded.
    pub fn waveform(&self) -> Vec<u16> {
        self.state.lock().map(|state| state.waveform.clone()).unwrap_or_default()
    }

    pub fn pause(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.playing = false;
        }
    }

    pub fn is_playing(&self) -> bool {
        is_playing(&self.state)
    }

    /// Seconds played.
    pub fn time(&self) -> f64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.length().map(|(_, rate)| state.position / f64::from(rate)))
            .unwrap_or(0.0)
    }

    /// The length, once decoded.
    pub fn duration(&self) -> Option<f64> {
        let state = self.state.lock().ok()?;
        state.length().map(|(frames, rate)| frames as f64 / f64::from(rate))
    }
}

/// The ring of an incoming call: a soft two-note chime every few seconds, until dropped.
#[derive(Debug)]
pub struct Ring {
    _stop: Sender<()>,
}

/// The ring's chime, at `t` seconds into its period: two notes that fade like a bell.
fn chime(t: f32) -> f32 {
    const PERIOD: f32 = 2.6;
    let t = t % PERIOD;
    let note = |start: f32, hz: f32| {
        let at = t - start;
        if !(0.0..1.2).contains(&at) {
            return 0.0;
        }
        let attack = (at / 0.01).min(1.0);
        attack * (-at * 4.5).exp() * (std::f32::consts::TAU * hz * at).sin()
    };
    0.22 * (note(0.0, 659.25) + note(0.22, 880.0))
}

impl Ring {
    /// Starts ringing on `device` (the default when `None`).
    pub fn start(device: Option<String>) -> Self {
        let (stop, stopped) = channel();
        let _ = std::thread::Builder::new().name("ring".into()).spawn(move || {
            let Some(device) = output_device(device.as_deref()) else { return };
            let mut clock = 0u64;
            let fill = move |out: &mut [f32], rate: u32, channels: usize| {
                // The clock wraps with the chime, so it stays exact however long it rings.
                let period = u64::from(rate) * 26 / 10;
                for frame in out.chunks_mut(channels) {
                    let sample = chime(clock as f32 / rate as f32);
                    frame.iter_mut().for_each(|out| *out = sample);
                    clock = (clock + 1) % period.max(1);
                }
            };
            if let Err(error) = play_on(&device, &stopped, fill) {
                eprintln!("better-matrix: {error}");
            }
        });
        Self { _stop: stop }
    }
}

// ----- input -------------------------------------------------------------------------------

/// An input level, smoothed like a meter: the peak jumps up at once and falls 20 dB a second,
/// the RMS rises over about 30 ms and falls over about 300 ms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    peak_db: f32,
    rms_db: f32,
}

impl Level {
    pub const SILENT: Level = Level { peak_db: FLOOR_DB, rms_db: FLOOR_DB };

    #[cfg(test)]
    pub fn peak_db(&self) -> f32 {
        self.peak_db
    }

    /// The RMS for a level bar: -60 dBFS and below is 0, full scale is 1.
    pub fn meter(&self) -> f32 {
        ((self.rms_db + 60.0) / 60.0).clamp(0.0, 1.0)
    }

    /// The level after a block of `samples` lasting `seconds`.
    fn after(self, samples: &[f32], seconds: f32) -> Self {
        let db = |amplitude: f32| if amplitude <= 1e-8 { FLOOR_DB } else { (20.0 * amplitude.log10()).max(FLOOR_DB) };
        let peak = db(samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs())));
        let rms = db((samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt());
        let fallen = self.peak_db - 20.0 * seconds;
        let speed = if rms > self.rms_db { 0.03 } else { 0.3 };
        let smoothed = self.rms_db + (rms - self.rms_db) * (1.0 - (-seconds / speed).exp());
        Self { peak_db: peak.max(fallen).max(FLOOR_DB), rms_db: smoothed.max(FLOOR_DB) }
    }
}

/// What a recording has heard so far.
#[derive(Debug, Default)]
struct Heard {
    /// Mono samples at `rate`.
    samples: Vec<f32>,
    rate: u32,
    level: Option<Level>,
    failed: Option<String>,
}

/// A finished recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    /// The file, whose extension follows its format.
    pub path: PathBuf,
    /// Seconds of audio.
    pub duration: f64,
    /// The loudness along it, [`WAVEFORM_POINTS`] amplitudes.
    pub waveform: Vec<u16>,
}

/// Handles share the recording: a clone stops the same one.
#[derive(Clone)]
pub struct NativeRecorder {
    path: PathBuf,
    device: Option<String>,
    heard: Arc<Mutex<Heard>>,
    live: rune::Live<f32>,
    stop: Arc<Mutex<Option<Sender<()>>>>,
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl std::fmt::Debug for NativeRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeRecorder").field("path", &self.path).finish_non_exhaustive()
    }
}

impl NativeRecorder {
    pub fn to_file(path: PathBuf, device: Option<String>) -> Self {
        Self {
            path,
            device,
            heard: Arc::default(),
            live: rune::Live::new(0.0),
            stop: Arc::default(),
            thread: Arc::default(),
        }
    }

    pub fn start(&self) {
        let (stop, stopped) = channel();
        if let Ok(mut slot) = self.stop.lock() {
            *slot = Some(stop);
        }
        let (heard, live, device) = (Arc::clone(&self.heard), self.live.clone(), self.device.clone());
        let thread = std::thread::Builder::new().name("audio-recorder".into()).spawn(move || {
            if let Err(error) = record_on(device.as_deref(), &stopped, &heard, live)
                && let Ok(mut heard) = heard.lock()
            {
                heard.failed = Some(error);
            }
        });
        if let (Ok(thread), Ok(mut slot)) = (thread, self.thread.lock()) {
            *slot = Some(thread);
        }
    }

    /// Stops recording, waiting for the last samples when `wait`.
    fn finish(&self, wait: bool) {
        if let Ok(mut slot) = self.stop.lock() {
            slot.take();
        }
        let thread = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(thread) = thread.filter(|_| wait) {
            let _ = thread.join();
        }
    }

    /// Stops and writes the file: 16-bit mono WAV. Blocking (it waits for the microphone to
    /// close and writes the file), so it runs off the UI thread.
    pub fn stop(&self) -> Result<Recording, String> {
        self.finish(true);
        let heard = self.heard.lock().map_err(|_| "the recording was lost".to_owned())?;
        if let Some(error) = &heard.failed {
            return Err(error.clone());
        }
        let samples = resample(&heard.samples, heard.rate, MEMO_RATE);
        let path = self.path.with_extension("wav");
        std::fs::write(&path, wav(&samples, MEMO_RATE)).map_err(|e| e.to_string())?;
        let duration = samples.len() as f64 / f64::from(MEMO_RATE);
        Ok(Recording { path, duration, waveform: waveform(&samples, 1, WAVEFORM_POINTS) })
    }

    pub fn cancel(&self) {
        self.finish(false);
    }

    /// Seconds recorded.
    pub fn elapsed(&self) -> f64 {
        self.heard.lock().map_or(0.0, |heard| heard.samples.len() as f64 / f64::from(heard.rate.max(1)))
    }

    /// Why recording stopped working (the microphone couldn't open, or went), if it did.
    pub fn failed(&self) -> Option<String> {
        self.heard.lock().ok().and_then(|heard| heard.failed.clone())
    }

    /// A recording whose microphone already failed, for tests.
    #[cfg(test)]
    pub fn failing(error: &str) -> Self {
        let recorder = Self::to_file(PathBuf::from("unused"), None);
        if let Ok(mut heard) = recorder.heard.lock() {
            heard.failed = Some(error.to_owned());
        }
        recorder
    }

    pub fn live_level(&self) -> rune::Live<f32> {
        self.live.clone()
    }
}

fn record_on(
    device: Option<&str>,
    stop: &Receiver<()>,
    heard: &Arc<Mutex<Heard>>,
    live: rune::Live<f32>,
) -> Result<(), String> {
    let device = input_device(device).ok_or("there is no microphone")?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let (format, config) = (supported.sample_format(), low_latency(&supported));
    if let Ok(mut heard) = heard.lock() {
        heard.rate = config.sample_rate;
        // A minute's room up front, so a short memo never moves its samples.
        heard.samples.reserve(config.sample_rate as usize * 60);
    }
    let stream = match format {
        SampleFormat::F32 => input_stream::<f32>(&device, config, Arc::clone(heard), live),
        SampleFormat::I16 => input_stream::<i16>(&device, config, Arc::clone(heard), live),
        SampleFormat::U16 => input_stream::<u16>(&device, config, Arc::clone(heard), live),
        SampleFormat::I32 => input_stream::<i32>(&device, config, Arc::clone(heard), live),
        other => Err(format!("the microphone's format {other} isn't supported")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    let _ = stop.recv();
    Ok(())
}

fn input_stream<T: SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    heard: Arc<Mutex<Heard>>,
    live: rune::Live<f32>,
) -> Result<cpal::Stream, String>
where
    f32: FromSample<T>,
{
    let (channels, rate) = (usize::from(config.channels).max(1), config.sample_rate);
    let mut block: Vec<f32> = Vec::new();
    let failed = Arc::clone(&heard);
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Mono: the channels averaged.
                block.clear();
                block.extend(
                    data.chunks(channels)
                        .map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32),
                );
                let Ok(mut heard) = heard.lock() else { return };
                let level = heard.level.unwrap_or(Level::SILENT).after(&block, block.len() as f32 / rate as f32);
                heard.level = Some(level);
                heard.samples.extend_from_slice(&block);
                live.set(level.meter());
            },
            // A microphone that goes fails the memo, rather than sending what came before.
            move |error: cpal::Error| {
                if device_lost(&error) {
                    if let Ok(mut heard) = failed.lock() {
                        heard.failed.get_or_insert_with(|| error.to_string());
                    }
                } else {
                    eprintln!("better-matrix: audio input: {error}");
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

/// `samples` from `from` to `to` Hz, between neighbouring samples (averaged when shrinking).
fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let count = (samples.len() as f64 / ratio) as usize;
    (0..count)
        .map(|i| {
            let start = i as f64 * ratio;
            let end = ((i + 1) as f64 * ratio).min(samples.len() as f64);
            let (a, b) = (start as usize, (end.ceil() as usize).max(start as usize + 1).min(samples.len()));
            samples[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// A 16-bit mono PCM WAV of `samples` at `rate`.
fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

// ----- what the app holds ------------------------------------------------------------------

/// A file to play: through the chosen speaker, or Rune's player when headless.
#[derive(Debug)]
pub enum Player {
    Rune(rune::Player),
    Native(NativePlayer),
}

impl Player {
    pub fn play(&self) {
        match self {
            Self::Rune(player) => player.play(),
            Self::Native(player) => player.play(),
        }
    }

    pub fn pause(&self) {
        match self {
            Self::Rune(player) => player.pause(),
            Self::Native(player) => player.pause(),
        }
    }

    pub fn is_playing(&self) -> bool {
        match self {
            Self::Rune(player) => player.is_playing(),
            Self::Native(player) => player.is_playing(),
        }
    }

    /// How far along it is, from 0 to 1; 0 until its length is known.
    pub fn progress(&self) -> f64 {
        match self {
            Self::Rune(player) => match player.duration() {
                Some(total) if total > 0.0 => (player.time() / total).clamp(0.0, 1.0),
                _ => 0.0,
            },
            Self::Native(player) => player.progress(),
        }
    }

    /// Moves to `share` of the way through: 0 the start, 1 the end.
    pub fn seek(&self, share: f64) {
        match self {
            Self::Rune(player) => {
                if let Some(duration) = player.duration() {
                    player.seek(share.clamp(0.0, 1.0) * duration);
                }
            }
            Self::Native(player) => player.seek(share),
        }
    }

    /// Plays `rate` times as fast, at the same pitch.
    pub fn set_rate(&self, rate: f64) {
        match self {
            Self::Rune(player) => player.set_rate(rate as f32),
            Self::Native(player) => player.set_rate(rate),
        }
    }

    /// The loudness along the file, once decoded; Rune's player doesn't say.
    pub fn waveform(&self) -> Vec<u16> {
        match self {
            Self::Rune(_) => Vec::new(),
            Self::Native(player) => player.waveform(),
        }
    }

    pub fn time(&self) -> f64 {
        match self {
            Self::Rune(player) => player.time(),
            Self::Native(player) => player.time(),
        }
    }

    pub fn duration(&self) -> Option<f64> {
        match self {
            Self::Rune(player) => player.duration(),
            Self::Native(player) => player.duration(),
        }
    }
}

/// A voice memo being recorded: from the chosen microphone, or Rune's recorder (with its fake
/// microphone) when headless.
#[derive(Clone, Debug)]
pub enum Recorder {
    Rune(rune::Recorder),
    Native(NativeRecorder),
}

impl Recorder {
    pub fn start(&self) {
        match self {
            Self::Rune(recorder) => recorder.start(),
            Self::Native(recorder) => recorder.start(),
        }
    }

    pub fn stop(&self) -> Result<Recording, String> {
        match self {
            // Rune's recorder keeps no samples: its file is read back for the waveform.
            Self::Rune(recorder) => recorder.stop().map(|done| {
                let waveform = decode(&done.path)
                    .map(|pcm| waveform(&pcm.samples, pcm.channels, WAVEFORM_POINTS))
                    .unwrap_or_default();
                Recording { path: done.path, duration: done.duration, waveform }
            }),
            Self::Native(recorder) => recorder.stop(),
        }
    }

    pub fn cancel(&self) {
        match self {
            Self::Rune(recorder) => recorder.cancel(),
            Self::Native(recorder) => recorder.cancel(),
        }
    }

    pub fn elapsed(&self) -> f64 {
        match self {
            Self::Rune(recorder) => recorder.elapsed(),
            Self::Native(recorder) => recorder.elapsed(),
        }
    }

    /// Why recording stopped working, if it did; Rune's fake microphone never fails.
    pub fn failed(&self) -> Option<String> {
        match self {
            Self::Rune(_) => None,
            Self::Native(recorder) => recorder.failed(),
        }
    }

    pub fn live_level(&self) -> rune::Live<f32> {
        match self {
            Self::Rune(recorder) => recorder.live_level(),
            Self::Native(recorder) => recorder.live_level(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_memo_should_shrink_to_its_rate_keeping_its_length() {
        let samples = vec![0.5; 48_000];
        let out = resample(&samples, 48_000, MEMO_RATE);
        assert_eq!(out.len(), 24_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn a_wav_should_carry_its_rate_and_samples() {
        let out = wav(&[0.0, 1.0], 24_000);
        assert_eq!((&out[..4], out.len()), (&b"RIFF"[..], 48));
        assert_eq!(u32::from_le_bytes([out[24], out[25], out[26], out[27]]), 24_000);
    }

    #[test]
    fn a_written_memo_should_decode_back() {
        let dir = std::env::temp_dir().join(format!("bm-sound-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("memo.wav");
        let tone: Vec<f32> = (0..2400).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        std::fs::write(&path, wav(&tone, MEMO_RATE)).unwrap_or_else(|_| panic!("unwritable"));
        let pcm = decode(&path).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!((pcm.rate, pcm.channels, pcm.frames()), (MEMO_RATE, 1, 2400));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_loud_block_should_raise_the_meter_at_once_and_silence_lower_it_slowly() {
        let loud = Level::SILENT.after(&[0.5; 480], 0.01);
        assert!(loud.peak_db() > -7.0);
        let after = loud.after(&[0.0; 480], 0.01);
        assert!(after.peak_db() > loud.peak_db() - 1.0);
    }

    #[test]
    fn a_waveform_should_show_loud_stretches_higher_than_quiet_ones() {
        let samples: Vec<f32> = (0..2000).map(|i| if i < 1000 { 0.001 } else { 0.5 }).collect();
        let points = waveform(&samples, 1, 2);
        assert!(points[0] < points[1] && points[1] <= matrix_core::AudioDetails::WAVEFORM_MAX);
    }

    #[test]
    fn a_waveform_of_silence_should_be_flat_at_zero() {
        assert_eq!(waveform(&[0.0; 400], 2, 4), vec![0; 4]);
    }

    fn tone(seconds: f32) -> Pcm {
        let samples = (0..(24_000.0 * seconds) as usize)
            .map(|i| (std::f32::consts::TAU * 300.0 * i as f32 / 24_000.0).sin() * 0.5);
        Pcm { rate: 24_000, channels: 1, samples: samples.collect() }
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    #[test]
    fn stretching_to_twice_the_speed_should_halve_the_length() {
        // Within half a window's hop (7.5 ms) of half its length.
        let fast = stretch(&tone(1.0), 2.0).unwrap_or_else(|| panic!("a second should stretch"));
        assert!(fast.frames().abs_diff(12_000) <= 180, "{} frames", fast.frames());
    }

    #[test]
    fn stretching_should_keep_both_ends_of_the_clip_audible() {
        for rate in [1.5, 2.0] {
            let fast = stretch(&tone(1.0), rate).unwrap_or_else(|| panic!("a second should stretch"));
            let (head, tail) = (&fast.samples[..120], &fast.samples[fast.samples.len() - 120..]);
            assert!(rms(head) > 0.25 && rms(tail) > 0.25, "at {rate}×: {} and {}", rms(head), rms(tail));
        }
    }

    #[test]
    fn a_clip_too_short_to_stretch_should_play_as_it_is() {
        assert!(stretch(&tone(0.08), 2.0).is_none());
    }

    fn playing(pcm: Pcm) -> Playback {
        Playback { pcm: Some(Arc::new(pcm)), playing: true, ..Playback::default() }
    }

    /// `pcm` at `rate`, with every segment stretched, or those `only` lists.
    fn stretched_only(pcm: &Pcm, rate: f64, only: Option<&[usize]>) -> Stretched {
        let mut stretched = Stretched::new(0, rate, pcm);
        for index in 0..stretched.segments.len() {
            if only.is_some_and(|only| !only.contains(&index)) {
                continue;
            }
            let bounds = stretched.bounds(index);
            let part = Pcm { rate: pcm.rate, channels: 1, samples: pcm.samples[bounds].to_vec() };
            stretched.segments[index] =
                stretch(&part, rate).map_or(Segment::Plain, |copy| Segment::Stretched(Arc::new(copy)));
        }
        stretched
    }

    fn stretched_all(pcm: &Pcm, rate: f64) -> Stretched {
        stretched_only(pcm, rate, None)
    }

    /// Zero crossings a second in `samples` at 24 kHz: 600 for a 300 Hz tone.
    fn crossings(samples: &[f32]) -> f32 {
        let count = samples.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        count as f32 / (samples.len() as f32 / 24_000.0)
    }

    #[test]
    fn a_seek_into_a_long_clip_should_play_at_speed_once_its_segment_is_stretched() {
        // 12 s in three segments; only the last is stretched, and the clip plays from inside it.
        let pcm = tone(12.0);
        let stretched = stretched_only(&pcm, 2.0, Some(&[2]));
        let mut clip = Playback { rate: 2.0, stretched: Some(stretched), position: 200_000.0, ..playing(pcm) };
        let mut out = vec![0.0; 12_000];
        clip.render(&mut out, 24_000, 1);
        // Half a second of output took a second of the file, at the tone's own pitch.
        assert!((clip.position - 224_000.0).abs() < 50.0, "at {}", clip.position);
        assert!((crossings(&out) - 600.0).abs() < 30.0, "{} crossings a second", crossings(&out));
    }

    #[test]
    fn stretched_segments_should_join_without_a_seam() {
        let pcm = tone(8.0);
        let mut clip =
            Playback { rate: 2.0, stretched: Some(stretched_all(&pcm, 2.0)), position: 86_000.0, ..playing(pcm) };
        // A quarter second of output either side of the join at 4 s.
        let mut out = vec![0.0; 12_000];
        clip.render(&mut out, 24_000, 1);
        let quietest = out.chunks(240).map(rms).fold(f32::MAX, f32::min);
        assert!(quietest > 0.25 && (crossings(&out) - 600.0).abs() < 30.0, "quietest {quietest}");
    }

    #[test]
    fn a_segment_join_heard_at_twice_the_file_s_rate_should_not_dip() {
        // A steady level stays steady through a stretched join, read at 48 kHz from 24 kHz.
        let pcm = Pcm { rate: 24_000, channels: 1, samples: vec![0.5; 24_000 * 8] };
        let mut clip =
            Playback { rate: 1.5, stretched: Some(stretched_all(&pcm, 1.5)), position: 90_000.0, ..playing(pcm) };
        let mut out = vec![0.0; 24_000];
        clip.render(&mut out, 48_000, 1);
        let lowest = out.iter().copied().fold(f32::MAX, f32::min);
        assert!(lowest > 0.49, "dips to {lowest}");
    }

    /// Eight seconds of smoothed noise: a sound whose stretched copy, unlike a pure tone's,
    /// isn't in step with the file at the same moment.
    fn murmur() -> Pcm {
        let (mut seed, mut level) = (12_345u32, 0.0f32);
        let samples = (0..24_000 * 8)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                level = level * 0.95 + noise * 0.4;
                level
            })
            .collect();
        Pcm { rate: 24_000, channels: 1, samples }
    }

    #[test]
    fn a_stretched_segment_arriving_mid_play_should_fade_in() {
        let pcm = murmur();
        let stretched = stretched_all(&pcm, 2.0);
        let mut clip = Playback { rate: 2.0, position: 10_000.0, ..playing(pcm) };
        clip.render(&mut [0.0; 480], 24_000, 1);
        // What playing on from the file would read next.
        let next = clip.pcm.as_ref().map_or(0.0, |pcm| pcm.sample(clip.position, 0));
        clip.stretched = Some(stretched);
        let mut after = [0.0; 1];
        clip.render(&mut after, 24_000, 1);
        assert!((after[0] - next).abs() < 1e-4, "jumps from {next} to {}", after[0]);
    }

    #[test]
    fn a_fade_begun_just_before_a_join_should_end_there() {
        // At 1.5× just short of the join at 4 s, then 2×: the fade from 1.5× would outlast the
        // join, where the 1.5× segment has nothing left to give.
        let pcm = murmur();
        let (faster, fastest) = (stretched_all(&pcm, 1.5), stretched_all(&pcm, 2.0));
        let mut clip = Playback { rate: 1.5, stretched: Some(faster), position: 95_900.0, ..playing(pcm) };
        clip.render(&mut [0.0; 8], 48_000, 1);
        clip.rate = 2.0;
        clip.stretched = Some(fastest);
        // Past the join, well within the 10 ms the fade would last.
        clip.render(&mut [0.0; 200], 48_000, 1);
        let alone = clip.source.as_ref().map_or(0.0, |source| source.read(clip.position, 0));
        let mut next = [0.0; 1];
        clip.render(&mut next, 48_000, 1);
        assert!((next[0] - alone).abs() < 1e-4, "plays {} where the new segment alone is {alone}", next[0]);
    }

    #[test]
    fn a_stretch_for_a_closed_player_or_an_older_job_should_stop() {
        for (closed, job) in [(true, 1), (false, 0)] {
            let pcm = Arc::new(tone(8.0));
            let mut playback = playing(tone(8.0));
            playback.jobs = 1;
            playback.stretched = Some(Stretched::new(1, 2.0, &pcm));
            playback.closed = closed;
            let state = Arc::new(Mutex::new(playback));
            if let Some(worker) = stretch_for(&state, pcm, job) {
                let _ = worker.join();
            }
            let untouched = state.lock().is_ok_and(|state| {
                state.stretched.as_ref().is_some_and(|s| s.segments.iter().all(|s| matches!(s, Segment::Pending)))
            });
            assert!(untouched, "closed {closed}, job {job}");
        }
    }

    /// Eight seconds of a steady voice-like sound at `pitch` Hz, with fading harmonics.
    fn vowel(pitch: f32) -> Pcm {
        let samples = (0..24_000 * 8)
            .map(|i| {
                let t = i as f32 / 24_000.0;
                (1..=4).map(|h| (std::f32::consts::TAU * pitch * h as f32 * t).sin() * 0.3 / h as f32).sum::<f32>()
            })
            .collect();
        Pcm { rate: 24_000, channels: 1, samples }
    }

    #[test]
    fn a_steady_voice_at_speed_should_keep_its_loudness_through_a_join() {
        // A high voice and a low one, at both speeds: a tenth of a second either side of the
        // join at 4 s, its loudness over one pitch period at every point.
        for (pitch, rate) in [(170.0, 1.5), (170.0, 2.0), (86.0, 1.5), (86.0, 2.0)] {
            let pcm = vowel(pitch);
            let stretched = Some(stretched_all(&pcm, rate));
            let start = 96_000.0 - 2_400.0 * rate;
            let mut clip = Playback { rate, stretched, position: start, ..playing(pcm) };
            let mut out = vec![0.0; 4_800];
            clip.render(&mut out, 24_000, 1);
            let period = (24_000.0 / pitch) as usize;
            let levels: Vec<f32> = out.windows(period).step_by(period / 4).map(rms).collect();
            let (quietest, loudest) =
                levels.iter().fold((f32::MAX, 0.0f32), |(q, l), level| (q.min(*level), l.max(*level)));
            let drop_db = 20.0 * (loudest / quietest).log10();
            assert!(drop_db < 2.0, "{pitch} Hz at {rate}×: dips {drop_db:.1} dB");
        }
    }

    #[test]
    fn the_segment_playing_should_be_stretched_first() {
        let mut stretched = Stretched::new(0, 2.0, &tone(16.0));
        let first = stretched.next(2);
        stretched.segments[2] = Segment::Plain;
        stretched.segments[3] = Segment::Plain;
        assert_eq!((first, stretched.next(2)), (Some(2), Some(0)));
    }

    #[test]
    fn a_clip_playing_to_its_end_should_stop_and_rewind() {
        let mut clip = playing(tone(0.01));
        clip.render(&mut [0.0; 480], 24_000, 1);
        assert_eq!((clip.playing, clip.position), (false, 0.0));
    }

    #[test]
    fn a_clip_paused_at_its_end_should_stay_there() {
        let mut clip = Playback { position: 240.0, playing: false, ..playing(tone(0.01)) };
        clip.render(&mut [0.0; 480], 24_000, 1);
        assert_eq!(clip.position, 240.0);
    }

    #[test]
    fn play_after_a_seek_to_the_end_before_decoding_should_start_from_the_beginning() {
        let player = NativePlayer { state: Arc::default(), wake: channel().0 };
        player.seek(1.0);
        let waiting = player.progress();
        player.play();
        assert_eq!((waiting, player.progress()), (1.0, 0.0));
    }

    /// A speaker for [`serve`] that counts how many are open.
    struct FakeSpeaker(Arc<std::sync::atomic::AtomicIsize>);

    impl Drop for FakeSpeaker {
        fn drop(&mut self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Waits up to two seconds for `done`.
    fn eventually(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "never {what}");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// A player on a fake speaker served on its own thread, with `release`: the player, how many
    /// speakers are open and have been opened, and how many times the file was loaded again.
    fn served(release: std::time::Duration) -> (NativePlayer, [Arc<std::sync::atomic::AtomicIsize>; 3]) {
        use std::sync::atomic::{AtomicIsize, Ordering};
        let pcm = tone(1.0);
        let waveform = waveform(&pcm.samples, 1, WAVEFORM_POINTS);
        let playback = Playback { pcm: Some(Arc::new(pcm)), waveform, ..Playback::default() };
        let (wake, woken) = channel();
        let player = NativePlayer { state: Arc::new(Mutex::new(playback)), wake };
        let counts: [Arc<AtomicIsize>; 3] = Default::default();
        let [open, opened, loads] = counts.clone();
        let state = Arc::clone(&player.state);
        std::thread::spawn(move || {
            let timing = PlayerTiming {
                poll: std::time::Duration::from_millis(1),
                linger: std::time::Duration::from_millis(5),
                release,
            };
            let speaker = || {
                open.fetch_add(1, Ordering::SeqCst);
                opened.fetch_add(1, Ordering::SeqCst);
                Ok((FakeSpeaker(Arc::clone(&open)), channel().1))
            };
            let load = || {
                loads.fetch_add(1, Ordering::SeqCst);
                let _ = state.lock().map(|mut state| state.install(Arc::new(tone(1.0))));
                Ok(())
            };
            serve(&state, &woken, timing, speaker, load)
        });
        (player, counts)
    }

    #[test]
    fn a_paused_clip_should_close_its_speaker_and_open_it_again_to_play() {
        use std::sync::atomic::Ordering;
        let (player, [open, opened, _]) = served(std::time::Duration::from_secs(60));
        let unopened = open.load(Ordering::SeqCst);
        player.play();
        eventually("opened", || open.load(Ordering::SeqCst) == 1);
        player.pause();
        eventually("closed", || open.load(Ordering::SeqCst) == 0);
        player.play();
        eventually("opened again", || opened.load(Ordering::SeqCst) == 2 && open.load(Ordering::SeqCst) == 1);
        assert_eq!(unopened, 0);
    }

    #[test]
    fn a_clip_played_to_its_end_should_close_its_speaker() {
        use std::sync::atomic::Ordering;
        let (player, [open, _, _]) = served(std::time::Duration::from_secs(60));
        player.play();
        eventually("opened", || open.load(Ordering::SeqCst) == 1);
        // The speaker plays the clip to its end.
        let _ = player.state.lock().map(|mut state| state.render(&mut vec![0.0; 48_000], 24_000, 1));
        eventually("closed", || open.load(Ordering::SeqCst) == 0);
        assert!(!player.is_playing());
    }

    #[test]
    fn a_clip_paused_a_while_should_let_its_samples_go_and_keep_its_place() {
        use std::sync::atomic::Ordering;
        let (player, [_, _, loads]) = served(std::time::Duration::from_millis(10));
        player.seek(0.5);
        eventually("released", || player.state.lock().is_ok_and(|state| state.pcm.is_none()));
        assert_eq!((player.progress(), player.duration(), player.time()), (0.5, Some(1.0), 0.5));
        assert_eq!(player.waveform().len(), WAVEFORM_POINTS);
        player.seek(0.25);
        player.play();
        eventually("loaded", || player.state.lock().is_ok_and(|state| state.pcm.is_some()));
        assert_eq!((loads.load(Ordering::SeqCst), player.progress()), (1, 0.25));
    }

    #[test]
    fn a_dropped_player_should_close_its_speaker() {
        use std::sync::atomic::Ordering;
        let (player, [open, _, _]) = served(std::time::Duration::from_secs(60));
        player.play();
        eventually("opened", || open.load(Ordering::SeqCst) == 1);
        drop(player);
        eventually("closed", || open.load(Ordering::SeqCst) == 0);
    }

    #[test]
    fn a_short_clip_at_twice_the_speed_should_keep_its_pitch_and_take_half_as_long() {
        // 120 ms of a 300 Hz tone, played at 2×: 600 zero crossings a second, whatever the speed.
        let pcm = tone(0.12);
        let stretched = stretched_all(&pcm, 2.0);
        let mut clip = Playback { rate: 2.0, stretched: Some(stretched), ..playing(pcm) };
        let mut out = vec![0.0; 2880];
        clip.render(&mut out, 24_000, 1);
        // It plays for 60 ms, half its length, then stops.
        let end = out.iter().rposition(|s| *s != 0.0).unwrap_or(0);
        let crossings = out[..end].windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let per_second = crossings as f32 / (end as f32 / 24_000.0);
        assert!(end.abs_diff(1440) <= 24 && !clip.playing, "played {end} frames");
        assert!((per_second - 600.0).abs() < 40.0, "{per_second} crossings a second");
    }

    #[test]
    fn stretching_should_keep_the_pitch() {
        // A 300 Hz tone crosses zero 600 times a second, at any speed.
        let tone: Vec<f32> =
            (0..48_000).map(|i| (std::f32::consts::TAU * 300.0 * i as f32 / 24_000.0).sin() * 0.5).collect();
        let fast = stretch(&Pcm { rate: 24_000, channels: 1, samples: tone }, 1.5)
            .unwrap_or_else(|| panic!("two seconds should stretch"));
        let middle = &fast.samples[2400..fast.samples.len() - 2400];
        let crossings = middle.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let per_second = crossings as f32 / (middle.len() as f32 / 24_000.0);
        assert!((per_second - 600.0).abs() < 30.0, "{per_second} crossings a second");
    }

    #[test]
    fn the_ring_should_be_quiet_between_chimes() {
        assert!(chime(0.05).abs() > 0.0);
        assert_eq!(chime(2.0), 0.0);
    }
}
