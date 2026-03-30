use binrw::BinReaderExt;
use downmixer::speaker_positions::{STEREO_LAYOUT, guess_channel_mask};
use downmixer::{Downmixer, DownmixerParams};
use eframe::egui::mutex::RwLock;
use either::{Either, Left, Right};
use itertools::Itertools;
use lazy_static::lazy_static;
use lewton::inside_ogg::OggStreamReader;
use linked_hash_map::LinkedHashMap;
use log::{error, warn};
use poll_promise::Promise;
use rodio::buffer::SamplesBuffer;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::io::{Cursor, Seek, SeekFrom};
use std::time::Instant;
use tiger_pkg::TagHash;
use tiger_pkg::package_manager;
use ww2ogg::{CodebookLibrary, WwiseRiffVorbis};

pub enum AudioPlayerState {
    Loading,
    Errored(String),
    Playing(PlayingFile),
}

pub struct AudioStreamInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample_count: u32,
}

pub type LoadedAudioFile = (Vec<i16>, AudioStreamInfo);

type AudioCacheMap = LinkedHashMap<
    TagHash,
    Either<Option<LoadedAudioFile>, Promise<Option<LoadedAudioFile>>>,
    BuildHasherDefault<FxHasher>,
>;

#[derive(Clone)]
pub struct PlayingFile {
    tag: TagHash,
    pub time: Instant,
    pub duration: f32,
}

pub struct AudioPlayer {
    cache: RwLock<AudioCacheMap>,
    _output: (rodio::OutputStream, rodio::OutputStreamHandle),
    sink: rodio::Sink,

    playing: RwLock<Option<PlayingFile>>,
}

unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

lazy_static! {
    static ref AUDIO_PLAYER_INSTANCE: AudioPlayer = AudioPlayer::new();
}

impl AudioPlayer {
    pub fn new() -> Self {
        let output = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&output.1).unwrap();
        sink.set_volume(0.5);
        Self {
            cache: RwLock::new(AudioCacheMap::default()),
            sink,
            _output: output,
            playing: RwLock::new(None),
        }
    }

    pub fn instance() -> &'static Self {
        &AUDIO_PLAYER_INSTANCE
    }
}

impl AudioPlayer {
    pub fn play(&self, hash: TagHash) -> AudioPlayerState {
        if hash.is_none() {
            return AudioPlayerState::Errored(format!("Tag {hash} is not linked to audio data"));
        }

        let mut ap = self.playing.write();
        // Already playing, don't restart playback
        if Some(hash) == ap.as_ref().map(|p| p.tag) {
            return AudioPlayerState::Playing(ap.as_ref().unwrap().clone());
        }

        let mut cache = self.cache.write();
        let c = cache.remove(&hash);
        let audio = if let Some(Left(r)) = c {
            r
        } else if let Some(Right(p)) = c {
            match p.try_take() {
                Ok(a) => a,
                Err(resume) => {
                    cache.insert(hash, Right(resume));
                    return AudioPlayerState::Loading;
                }
            }
        } else if c.is_none() {
            cache.insert(
                hash,
                Right(Promise::spawn_async(Self::load_audio_task(hash))),
            );

            return AudioPlayerState::Loading;
        } else {
            return AudioPlayerState::Loading;
        };

        let state = if let Some((samples, desc)) = &audio {
            let sb = SamplesBuffer::new(2, desc.sample_rate, samples.to_vec());
            self.sink.stop();
            self.sink.clear();
            self.sink.append(sb);
            self.sink.play();

            let duration = desc.sample_count as f32 / desc.sample_rate as f32;
            let playing = PlayingFile {
                tag: hash,
                time: Instant::now(),
                duration,
            };

            *ap = Some(playing.clone());

            AudioPlayerState::Playing(playing)
        } else {
            AudioPlayerState::Errored(
                "Failed to load audio file, check console for more information".to_string(),
            )
        };

        cache.insert(hash, Left(audio));

        drop(cache);
        self.truncate();

        state
    }

    pub fn stop(&self) {
        self.playing.write().take();
        self.sink.stop();
    }

    async fn load_audio_task(hash: TagHash) -> Option<LoadedAudioFile> {
        let data = package_manager().read_tag(hash).ok()?;
        let reader = Cursor::new(data);

        let mut converter =
            match WwiseRiffVorbis::new(reader, CodebookLibrary::aotuv_codebooks().unwrap()) {
                Ok(o) => o,
                Err(e) => {
                    error!("Failed to open wwise stream file {hash}: {e}");
                    return None;
                }
            };

        let info = AudioStreamInfo {
            channels: converter.num_channels(),
            sample_rate: converter.sample_rate(),
            sample_count: converter.sample_count(),
        };

        let mut ogg_data = vec![];
        if let Err(e) = converter.generate_ogg(&mut ogg_data) {
            error!("Failed to generate ogg for {hash}: {e}");
            return None;
        }

        let mut ogg_reader = match OggStreamReader::new(Cursor::new(ogg_data)) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to create ogg reader for {hash}: {e}");
                return None;
            }
        };

        let mut samples = Vec::with_capacity(info.sample_count as usize);
        let mixer = Downmixer::new(
            guess_channel_mask(info.channels).unwrap_or(STEREO_LAYOUT),
            DownmixerParams::default(),
        );
        while let Some(packet) = ogg_reader.read_dec_packet_itl().ok()? {
            for frame in packet.chunks_exact(info.channels as usize) {
                let (left, right) = mixer.downmix_frame_to_stereo(frame);
                samples.push(left);
                samples.push(right);
            }
        }

        Some((samples, info))
    }

    const MAX_FILES: usize = 64;
    fn truncate(&self) {
        let mut cache = self.cache.write();
        while cache.len() > Self::MAX_FILES {
            cache.pop_front();
        }
    }
}

pub fn get_stream_duration_fast(tag: TagHash) -> f32 {
    let Ok(data) = package_manager().read_tag(tag) else {
        return 0.0;
    };

    if data.len() < 0x20 {
        return 0.0;
    }

    let mut cur = Cursor::new(data);

    let magic: [u8; 4] = cur.read_le().unwrap();

    match &magic {
        b"RIFF" => {
            cur.seek(SeekFrom::Start(0x4)).ok();
            let data_size = cur.read_le::<u32>().unwrap();
            cur.seek(SeekFrom::Start(0x1c)).ok();
            let byte_rate = cur.read_le::<u32>().unwrap();

            (data_size as f64 / byte_rate as f64) as f32
        }
        b"RIFX" => {
            cur.seek(SeekFrom::Start(0x4)).ok();
            let data_size = cur.read_le::<u32>().unwrap();
            cur.seek(SeekFrom::Start(0x1c)).ok();
            let byte_rate = cur.read_be::<u32>().unwrap();

            (data_size as f64 / byte_rate as f64) as f32
        }
        _ => {
            warn!(
                "Unknown audio file format 0x{:02X}{:02X}{:02X}{:02X} for tag {tag}",
                magic[0], magic[1], magic[2], magic[3]
            );
            0.0
        }
    }
}
