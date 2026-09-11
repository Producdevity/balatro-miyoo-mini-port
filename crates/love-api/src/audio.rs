use anyhow::{Context, Result};
use lewton::inside_ogg::OggStreamReader;
use mlua::prelude::*;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Weak};
use std::time::Duration;

use crate::state::SharedState;

mod cadence;
mod metrics;
mod onion;
mod output;
mod pcm;
mod preparation;
mod prepare;
mod scheduling;
mod source_cache;
use output::AudioOutput;
pub use prepare::{prepare_cache, prepare_cache_with_progress, CacheProgress, CacheSummary};

const OUTPUT_RATE: u32 = 44_100;
const MIX_FRAMES: usize = 256;
const STATIC_CACHE_BYTES: usize = 8 * 1024 * 1024;
const STATIC_CLIP_BYTES: usize = 512 * 1024;

struct SourceControl {
    id: u64,
    playing: AtomicBool,
    paused: AtomicBool,
    looping: AtomicBool,
    volume: AtomicU32,
    pitch: AtomicU32,
}

impl SourceControl {
    fn new(id: u64) -> Self {
        Self {
            id,
            playing: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            looping: AtomicBool::new(false),
            volume: AtomicU32::new(1.0f32.to_bits()),
            pitch: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed)).clamp(0.0, 2.0)
    }

    fn pitch(&self) -> f64 {
        f32::from_bits(self.pitch.load(Ordering::Relaxed)).clamp(0.1, 4.0) as f64
    }
}

#[derive(Clone)]
struct SourceDescription {
    path: String,
    streaming: bool,
    data: Arc<[u8]>,
    control: Arc<SourceControl>,
}

enum AudioCommand {
    Play(u64),
    PlayMany(Vec<u64>),
    Stop(u64),
    Release(u64),
    StopAll,
}

enum AudioRequest {
    Register(SourceDescription),
    Control(AudioCommand),
}

enum MixerMessage {
    Register(u64, RegisteredSource),
    Control(AudioCommand),
}

struct AudioEngine {
    enabled: bool,
    sender: Option<mpsc::Sender<AudioRequest>>,
    next_id: AtomicU64,
    master_volume: Arc<AtomicU32>,
    controls: Mutex<HashMap<u64, Weak<SourceControl>>>,
    source_data: Mutex<source_cache::SourceCache>,
}

impl AudioEngine {
    fn new() -> Arc<Self> {
        let enabled = std::env::var("BALATRO_AUDIO").as_deref() == Ok("1");
        let (sender, receiver) = mpsc::channel();
        let engine = Arc::new(Self {
            enabled,
            sender: enabled.then_some(sender),
            next_id: AtomicU64::new(1),
            master_volume: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            controls: Mutex::new(HashMap::new()),
            source_data: Mutex::new(source_cache::SourceCache::default()),
        });
        if enabled {
            let (prepared_sender, prepared_receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("balatro-sounds".to_owned())
                .stack_size(256 * 1024)
                .spawn(move || preparation::run(receiver, prepared_sender))
                .expect("start sound loader");
            let volume = Arc::clone(&engine.master_volume);
            std::thread::Builder::new()
                .name("balatro-audio".to_owned())
                .stack_size(256 * 1024)
                .spawn(move || audio_worker(prepared_receiver, volume))
                .expect("start audio worker");
        } else {
            eprintln!("[audio] disabled");
        }
        engine
    }

    fn new_source(&self, path: String, streaming: bool, data: Arc<[u8]>) -> SourceDescription {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let control = Arc::new(SourceControl::new(id));
        let description = SourceDescription {
            path,
            streaming,
            data,
            control: Arc::clone(&control),
        };
        self.controls.lock().insert(id, Arc::downgrade(&control));
        if let Some(sender) = &self.sender {
            let _ = sender.send(AudioRequest::Register(description.clone()));
        }
        description
    }

    fn play(&self, id: u64) {
        if let Some(control) = self.controls.lock().get(&id).and_then(Weak::upgrade) {
            control.paused.store(false, Ordering::Relaxed);
            control.playing.store(self.enabled, Ordering::Relaxed);
        }
        self.send(AudioCommand::Play(id));
    }

    fn stop(&self, id: u64) {
        if let Some(control) = self.controls.lock().get(&id).and_then(Weak::upgrade) {
            control.playing.store(false, Ordering::Relaxed);
            control.paused.store(false, Ordering::Relaxed);
        }
        self.send(AudioCommand::Stop(id));
    }

    fn play_many(&self, ids: Vec<u64>) -> bool {
        let controls = self.controls.lock();
        if ids
            .iter()
            .any(|id| controls.get(id).and_then(Weak::upgrade).is_none())
        {
            return false;
        }
        for id in &ids {
            if let Some(control) = controls[id].upgrade() {
                control.paused.store(false, Ordering::Relaxed);
                control.playing.store(self.enabled, Ordering::Relaxed);
            }
        }
        self.send(AudioCommand::PlayMany(ids));
        self.enabled
    }

    fn pause(&self, id: u64) {
        if let Some(control) = self.controls.lock().get(&id).and_then(Weak::upgrade) {
            control.paused.store(true, Ordering::Relaxed);
        }
    }

    fn release(&self, id: u64) {
        if let Some(control) = self
            .controls
            .lock()
            .remove(&id)
            .and_then(|control| control.upgrade())
        {
            control.playing.store(false, Ordering::Relaxed);
        }
        self.send(AudioCommand::Release(id));
    }

    fn stop_all(&self) {
        for control in self.controls.lock().values().filter_map(Weak::upgrade) {
            control.playing.store(false, Ordering::Relaxed);
            control.paused.store(false, Ordering::Relaxed);
        }
        self.send(AudioCommand::StopAll);
    }

    fn active_count(&self) -> usize {
        let mut controls = self.controls.lock();
        controls.retain(|_, control| control.strong_count() > 0);
        controls
            .values()
            .filter_map(Weak::upgrade)
            .filter(|control| control.playing.load(Ordering::Relaxed))
            .count()
    }

    fn send(&self, command: AudioCommand) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(AudioRequest::Control(command));
        }
    }
}

#[derive(Clone)]
struct AudioClip {
    samples: Arc<[i16]>,
    channels: usize,
    sample_rate: u32,
}

#[derive(Clone)]
enum AudioAsset {
    Static(AudioClip),
    Stream(Arc<[u8]>),
    Pcm(Arc<pcm::Asset>),
}

struct RegisteredSource {
    asset: AudioAsset,
    control: Weak<SourceControl>,
}

struct StaticCache {
    clips: HashMap<String, AudioClip>,
    order: VecDeque<String>,
    bytes: usize,
}

impl StaticCache {
    fn new() -> Self {
        Self {
            clips: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&self, path: &str) -> Option<AudioClip> {
        self.clips.get(path).cloned()
    }

    fn insert(&mut self, path: String, clip: AudioClip) {
        let bytes = clip.samples.len() * std::mem::size_of::<i16>();
        if bytes > STATIC_CACHE_BYTES / 4 || self.clips.contains_key(&path) {
            return;
        }
        while self.bytes + bytes > STATIC_CACHE_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.clips.remove(&oldest) {
                self.bytes -= old.samples.len() * std::mem::size_of::<i16>();
            }
        }
        self.bytes += bytes;
        self.order.push_back(path.clone());
        self.clips.insert(path, clip);
    }
}

type VorbisReader = OggStreamReader<Cursor<Arc<[u8]>>>;

struct StreamPlayback {
    data: Arc<[u8]>,
    reader: VorbisReader,
    samples: Vec<i16>,
    channels: usize,
    sample_rate: u32,
    eof: bool,
    decode_time: Duration,
}

impl StreamPlayback {
    fn new(data: Arc<[u8]>) -> Result<Self> {
        let reader = OggStreamReader::new(Cursor::new(Arc::clone(&data)))?;
        let channels = reader.ident_hdr.audio_channels as usize;
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        Ok(Self {
            data,
            reader,
            samples: Vec::with_capacity(8192 * channels),
            channels,
            sample_rate,
            eof: false,
            decode_time: Duration::ZERO,
        })
    }

    fn sample(&mut self, frame: usize) -> Result<Option<(i16, i16)>> {
        while self.samples.len() / self.channels <= frame && !self.eof {
            let started = std::time::Instant::now();
            let packet = self.reader.read_dec_packet_itl()?;
            self.decode_time += started.elapsed();
            match packet {
                Some(packet) => self.samples.extend(packet),
                None => self.eof = true,
            }
        }
        let offset = frame * self.channels;
        if offset >= self.samples.len() {
            return Ok(None);
        }
        let left = self.samples[offset];
        let right = if self.channels > 1 {
            self.samples[offset + 1]
        } else {
            left
        };
        Ok(Some((left, right)))
    }

    fn trim(&mut self, position: &mut f64) {
        let frames = position.floor() as usize;
        if frames < 4096 {
            return;
        }
        let samples = frames * self.channels;
        self.samples.drain(..samples.min(self.samples.len()));
        *position -= frames as f64;
    }

    fn restart(&mut self) -> Result<()> {
        *self = Self::new(Arc::clone(&self.data))?;
        Ok(())
    }
}

enum Playback {
    Static(AudioClip),
    Stream(Box<StreamPlayback>),
    Pcm(Box<pcm::Playback>),
}

struct Voice {
    id: u64,
    control: Arc<SourceControl>,
    playback: Playback,
    position: f64,
}

impl Voice {
    fn new(id: u64, source: &RegisteredSource) -> Result<Self> {
        let control = source
            .control
            .upgrade()
            .context("audio source was released")?;
        let playback = match &source.asset {
            AudioAsset::Static(clip) => Playback::Static(clip.clone()),
            AudioAsset::Stream(data) => {
                Playback::Stream(Box::new(StreamPlayback::new(Arc::clone(data))?))
            }
            AudioAsset::Pcm(asset) => Playback::Pcm(Box::new(pcm::Playback::new(asset)?)),
        };
        Ok(Self {
            id,
            control,
            playback,
            position: 0.0,
        })
    }

    fn sample(&mut self, muted: bool) -> Result<Option<(f32, f32)>> {
        let frame = self.position as usize;
        if muted {
            if let Playback::Pcm(stream) = &self.playback {
                return Ok((frame < stream.frames()).then_some((0.0, 0.0)));
            }
        }
        let Some((left, right)) = self.sample_frame(frame)? else {
            return Ok(None);
        };
        let fraction = (self.position - frame as f64) as f32;
        if fraction == 0.0 {
            return Ok(Some((left as f32, right as f32)));
        }
        let (next_left, next_right) = self.sample_frame(frame + 1)?.unwrap_or((left, right));
        Ok(Some((
            left as f32 + (next_left as f32 - left as f32) * fraction,
            right as f32 + (next_right as f32 - right as f32) * fraction,
        )))
    }

    fn sample_frame(&mut self, frame: usize) -> Result<Option<(i16, i16)>> {
        match &mut self.playback {
            Playback::Static(clip) => {
                let offset = frame * clip.channels;
                if offset >= clip.samples.len() {
                    return Ok(None);
                }
                let left = clip.samples[offset];
                let right = if clip.channels > 1 {
                    clip.samples[offset + 1]
                } else {
                    left
                };
                Ok(Some((left, right)))
            }
            Playback::Stream(stream) => stream.sample(frame),
            Playback::Pcm(stream) => stream.sample(frame),
        }
    }

    fn sample_rate(&self) -> u32 {
        match &self.playback {
            Playback::Static(clip) => clip.sample_rate,
            Playback::Stream(stream) => stream.sample_rate,
            Playback::Pcm(stream) => stream.sample_rate(),
        }
    }

    fn restart(&mut self) -> Result<()> {
        self.position = 0.0;
        if let Playback::Stream(stream) = &mut self.playback {
            stream.restart()?;
        }
        Ok(())
    }

    fn trim_stream(&mut self) {
        if let Playback::Stream(stream) = &mut self.playback {
            stream.trim(&mut self.position);
        }
    }
}

#[cfg(test)]
fn decode_static(data: Arc<[u8]>) -> Result<AudioClip> {
    let mut reader = OggStreamReader::new(Cursor::new(data))?;
    let channels = reader.ident_hdr.audio_channels as usize;
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl()? {
        samples.extend(packet);
    }
    Ok(AudioClip {
        samples: samples.into(),
        channels,
        sample_rate,
    })
}

fn decode_asset(data: Arc<[u8]>) -> Result<AudioAsset> {
    let mut reader = OggStreamReader::new(Cursor::new(Arc::clone(&data)))?;
    let channels = reader.ident_hdr.audio_channels as usize;
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let mut samples = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl()? {
        if samples.len() + packet.len() > STATIC_CLIP_BYTES / 2 {
            return Ok(AudioAsset::Stream(data));
        }
        samples.extend(packet);
    }
    Ok(AudioAsset::Static(AudioClip {
        samples: samples.into(),
        channels,
        sample_rate,
    }))
}

fn handle_command(
    command: AudioCommand,
    registered: &mut HashMap<u64, RegisteredSource>,
    voices: &mut Vec<Voice>,
) {
    match command {
        AudioCommand::PlayMany(ids) => {
            for id in ids {
                handle_command(AudioCommand::Play(id), registered, voices);
            }
        }
        AudioCommand::Play(id) => {
            if voices.iter().any(|voice| voice.id == id) {
                return;
            }
            let Some(source) = registered.get(&id) else {
                return;
            };
            match Voice::new(id, source) {
                Ok(voice) => voices.push(voice),
                Err(error) => {
                    eprintln!("[audio] start source {id}: {error:#}");
                    if let Some(control) = source.control.upgrade() {
                        control.playing.store(false, Ordering::Relaxed);
                    }
                }
            }
        }
        AudioCommand::Stop(id) => voices.retain(|voice| voice.id != id),
        AudioCommand::Release(id) => {
            voices.retain(|voice| voice.id != id);
            registered.remove(&id);
        }
        AudioCommand::StopAll => voices.clear(),
    }
    registered.retain(|_, source| source.control.strong_count() > 0);
}

fn mix(voices: &mut Vec<Voice>, master_volume: f32, output: &mut [i16], accumulator: &mut [i32]) {
    accumulator.fill(0);
    for voice in voices.iter_mut() {
        if voice.control.paused.load(Ordering::Relaxed) {
            continue;
        }
        let volume = voice.control.volume() * master_volume;
        let step = voice.control.pitch() * voice.sample_rate() as f64 / OUTPUT_RATE as f64;
        let mut active = true;
        for frame in 0..output.len() / 2 {
            let sample = match voice.sample(volume == 0.0) {
                Ok(Some(sample)) => Some(sample),
                Ok(None) if voice.control.looping.load(Ordering::Relaxed) => {
                    if voice.restart().is_ok() {
                        voice.sample(volume == 0.0).ok().flatten()
                    } else {
                        None
                    }
                }
                Ok(None) => None,
                Err(error) => {
                    eprintln!("[audio] source {}: {error:#}", voice.id);
                    None
                }
            };
            let Some((left, right)) = sample else {
                active = false;
                break;
            };
            accumulator[frame * 2] += (left * volume) as i32;
            accumulator[frame * 2 + 1] += (right * volume) as i32;
            voice.position += step;
        }
        if active {
            voice.trim_stream();
        } else {
            voice.control.playing.store(false, Ordering::Relaxed);
        }
    }
    voices.retain(|voice| voice.control.playing.load(Ordering::Relaxed));
    for (sample, mixed) in output.iter_mut().zip(accumulator.iter().copied()) {
        *sample = mixed.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
}

fn handle_message(
    message: MixerMessage,
    registered: &mut HashMap<u64, RegisteredSource>,
    voices: &mut Vec<Voice>,
) {
    match message {
        MixerMessage::Register(id, source) => {
            registered.insert(id, source);
        }
        MixerMessage::Control(command) => handle_command(command, registered, voices),
    }
}

fn audio_worker(receiver: mpsc::Receiver<MixerMessage>, master_volume: Arc<AtomicU32>) {
    scheduling::configure();
    let mut output = match AudioOutput::open() {
        Ok(output) => output,
        Err(error) => {
            eprintln!("[audio] output unavailable: {error:#}");
            while let Ok(command) = receiver.recv() {
                if let MixerMessage::Register(_, source) = command {
                    if let Some(control) = source.control.upgrade() {
                        control.playing.store(false, Ordering::Relaxed);
                    }
                }
            }
            return;
        }
    };
    let mut registered = HashMap::new();
    let mut voices = Vec::new();
    let mut samples = vec![0i16; MIX_FRAMES * 2];
    let mut accumulator = vec![0i32; MIX_FRAMES * 2];
    eprintln!("[audio] 44100 Hz stereo, {MIX_FRAMES}-frame mix blocks");
    let mut metrics = metrics::AudioMetrics::new();
    let mut cadence = cadence::Cadence::default();
    let mut output_frame = 0;

    loop {
        if voices.is_empty() {
            cadence.reset();
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(command) => handle_message(command, &mut registered, &mut voices),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    registered.retain(|_, source| source.control.strong_count() > 0);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        let work_started = std::time::Instant::now();
        while let Ok(command) = receiver.try_recv() {
            handle_message(command, &mut registered, &mut voices);
        }
        let command_time = work_started.elapsed();
        let master = f32::from_bits(master_volume.load(Ordering::Relaxed)).clamp(0.0, 1.0);
        metrics.record_starts(&voices, output_frame);
        mix(&mut voices, master, &mut samples, &mut accumulator);
        output_frame += MIX_FRAMES as u64;
        let mut decode_time = Duration::ZERO;
        for voice in &mut voices {
            if let Playback::Stream(stream) = &mut voice.playback {
                decode_time += std::mem::take(&mut stream.decode_time);
            }
        }
        metrics.record(
            work_started.elapsed(),
            command_time,
            decode_time,
            voices.len(),
            voices
                .iter()
                .filter(|voice| {
                    matches!(voice.playback, Playback::Stream(_) | Playback::Pcm(_))
                        && voice.control.volume() == 0.0
                })
                .count(),
            voices
                .iter()
                .filter(|voice| voice.control.volume().is_subnormal())
                .count(),
        );
        registered.retain(|_, source| source.control.strong_count() > 0);
        let mixed = std::time::Instant::now();
        if let Err(error) = output.write_samples(&samples) {
            eprintln!("[audio] write failed: {error:#}");
            for voice in &voices {
                voice.control.playing.store(false, Ordering::Relaxed);
            }
            return;
        }
        if let Some(report) =
            cadence.record(work_started, mixed, std::time::Instant::now(), MIX_FRAMES)
        {
            let music = voices
                .iter()
                .filter(|voice| {
                    matches!(voice.playback, Playback::Stream(_) | Playback::Pcm(_))
                        && voice.control.volume() > 0.01
                })
                .max_by(|a, b| a.control.volume().total_cmp(&b.control.volume()));
            eprintln!(
                "[audio-cadence] frames_per_second={:.0} expected={} mix_max_ms={:.3} write_max_ms={:.3} stream={} pitch={:.3}",
                report.frames_per_second, OUTPUT_RATE,
                report.max_mix.as_secs_f64() * 1000.0,
                report.max_write.as_secs_f64() * 1000.0,
                music.map_or(0, |voice| voice.id),
                music.map_or(0.0, |voice| voice.control.pitch()),
            );
        }
    }
}

fn source_table(
    lua: &Lua,
    engine: Arc<AudioEngine>,
    description: SourceDescription,
) -> LuaResult<LuaTable> {
    let control = Arc::clone(&description.control);
    let source = lua.create_table()?;
    source.set("_svmm_source_id", control.id)?;
    {
        let engine = Arc::clone(&engine);
        let control = Arc::clone(&control);
        source.set(
            "play",
            lua.create_function(move |_, _self: LuaValue| {
                engine.play(control.id);
                Ok(())
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        let control = Arc::clone(&control);
        source.set(
            "stop",
            lua.create_function(move |_, _self: LuaValue| {
                engine.stop(control.id);
                Ok(())
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        let control = Arc::clone(&control);
        source.set(
            "pause",
            lua.create_function(move |_, _self: LuaValue| {
                engine.pause(control.id);
                Ok(())
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "setVolume",
            lua.create_function(move |_, (_self, volume): (LuaValue, f32)| {
                control.volume.store(volume.to_bits(), Ordering::Relaxed);
                Ok(())
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "getVolume",
            lua.create_function(move |_, _self: LuaValue| Ok(control.volume() as f64))?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "setLooping",
            lua.create_function(move |_, (_self, looping): (LuaValue, bool)| {
                control.looping.store(looping, Ordering::Relaxed);
                Ok(())
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "isLooping",
            lua.create_function(move |_, _self: LuaValue| {
                Ok(control.looping.load(Ordering::Relaxed))
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "isPlaying",
            lua.create_function(move |_, _self: LuaValue| {
                Ok(control.playing.load(Ordering::Relaxed)
                    && !control.paused.load(Ordering::Relaxed))
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "isStopped",
            lua.create_function(move |_, _self: LuaValue| {
                Ok(!control.playing.load(Ordering::Relaxed))
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "isPaused",
            lua.create_function(move |_, _self: LuaValue| {
                Ok(control.paused.load(Ordering::Relaxed))
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "setPitch",
            lua.create_function(move |_, (_self, pitch): (LuaValue, f32)| {
                control.pitch.store(pitch.to_bits(), Ordering::Relaxed);
                Ok(())
            })?,
        )?;
    }
    {
        let control = Arc::clone(&control);
        source.set(
            "getPitch",
            lua.create_function(move |_, _self: LuaValue| Ok(control.pitch()))?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        let path = description.path.clone();
        let streaming = description.streaming;
        let data = Arc::clone(&description.data);
        source.set(
            "clone",
            lua.create_function(move |lua, _self: LuaValue| {
                let clone = engine.new_source(path.clone(), streaming, Arc::clone(&data));
                source_table(lua, Arc::clone(&engine), clone)
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        let control = Arc::clone(&control);
        source.set(
            "release",
            lua.create_function(move |_, _self: LuaValue| {
                engine.release(control.id);
                Ok(())
            })?,
        )?;
    }
    source.set(
        "seek",
        lua.create_function(|_, _args: LuaMultiValue| Ok(()))?,
    )?;
    source.set(
        "tell",
        lua.create_function(|_, _self: LuaValue| Ok(0.0f64))?,
    )?;
    source.set(
        "type",
        lua.create_function(|_, _self: LuaValue| Ok("Source"))?,
    )?;
    source.set(
        "typeOf",
        lua.create_function(|_, (_self, name): (LuaValue, String)| {
            Ok(name == "Source" || name == "Object")
        })?,
    )?;
    Ok(source)
}

fn source_id(value: &LuaValue) -> Option<u64> {
    match value {
        LuaValue::Table(source) => source.get("_svmm_source_id").ok(),
        _ => None,
    }
}

fn playback_ids(values: LuaMultiValue) -> LuaResult<Vec<u64>> {
    let values = if values.len() == 1 && source_id(&values[0]).is_none() {
        match &values[0] {
            LuaValue::Table(list) => list
                .sequence_values::<LuaValue>()
                .collect::<LuaResult<Vec<_>>>()?,
            _ => {
                return Err(LuaError::runtime(
                    "expected an audio source or list of sources",
                ))
            }
        }
    } else {
        values.into_vec()
    };
    values
        .iter()
        .map(|value| source_id(value).ok_or_else(|| LuaError::runtime("expected an audio source")))
        .collect()
}

pub fn register(lua: &Lua, love: &LuaTable, state: Arc<SharedState>) -> LuaResult<()> {
    let engine = AudioEngine::new();
    let audio = lua.create_table()?;

    {
        let engine = Arc::clone(&engine);
        audio.set(
            "newSource",
            lua.create_function(move |lua, args: LuaMultiValue| {
                let path = match args.front() {
                    Some(LuaValue::String(path)) => path.to_string_lossy().to_string(),
                    _ => return Err(LuaError::runtime("audio source path is required")),
                };
                let streaming = matches!(
                    args.get(1),
                    Some(LuaValue::String(kind)) if kind.as_bytes() == b"stream"
                );
                let data = engine
                    .source_data
                    .lock()
                    .get_or_load(&path, || state.game_source.lock().read_file(&path))
                    .map_err(LuaError::external)?;
                let description = engine.new_source(path, streaming, data);
                source_table(lua, Arc::clone(&engine), description)
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "play",
            lua.create_function(move |_, values: LuaMultiValue| {
                if values.len() == 1 {
                    if let Some(id) = source_id(&values[0]) {
                        engine.play(id);
                        return Ok(engine.enabled);
                    }
                }
                let ids = playback_ids(values)?;
                Ok(engine.play_many(ids))
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "stop",
            lua.create_function(move |_, value: Option<LuaValue>| {
                if let Some(id) = value.as_ref().and_then(source_id) {
                    engine.stop(id);
                } else {
                    engine.stop_all();
                }
                Ok(())
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "pause",
            lua.create_function(move |_, value: LuaValue| {
                if let Some(id) = source_id(&value) {
                    engine.pause(id);
                }
                Ok(())
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "setVolume",
            lua.create_function(move |_, volume: f32| {
                engine
                    .master_volume
                    .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
                Ok(())
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "getVolume",
            lua.create_function(move |_, ()| {
                Ok(f32::from_bits(engine.master_volume.load(Ordering::Relaxed)) as f64)
            })?,
        )?;
    }
    {
        let engine = Arc::clone(&engine);
        audio.set(
            "getActiveSourceCount",
            lua.create_function(move |_, ()| Ok(engine.active_count() as i32))?,
        )?;
    }

    love.set("audio", audio)?;
    Ok(())
}

#[cfg(test)]
mod tests;
