use crate::gui::texture::{LoadedTexture, Texture};
use crate::package_manager::package_manager;
use destiny_pkg::TagHash;
use eframe::egui::mutex::RwLock;
use eframe::egui::TextureId;
use eframe::egui_wgpu::RenderState;
use eframe::wgpu;
use either::{Either, Left, Right};
use lazy_static::lazy_static;
use linked_hash_map::LinkedHashMap;
use log::error;
use poll_promise::Promise;
use rodio::buffer::SamplesBuffer;
use rodio::Source;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vgmstream::info::VgmstreamInfo;

pub enum AudioPlayerState {
    Loading,
    Errored(String),
    Playing(PlayingFile),
}
pub type LoadedAudioFile = (Vec<i16>, VgmstreamInfo);

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
    cache: Arc<RwLock<AudioCacheMap>>,
    output: (rodio::OutputStream, rodio::OutputStreamHandle),
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
            cache: Arc::new(RwLock::new(AudioCacheMap::default())),
            sink,
            output,
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
            let sb = SamplesBuffer::new(
                desc.channels as u16,
                desc.sample_rate as u32,
                samples.to_vec(),
            );
            self.sink.stop();
            self.sink.clear();
            self.sink.append(sb);
            self.sink.play();

            let duration = samples.len() as f32 / (desc.channels as f32 * desc.sample_rate as f32);
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

        let filename = format!(".\\{hash}.wem");
        let (samples, desc) = match vgmstream::read_file_to_samples(&data, Some(filename)) {
            Ok(o) => o,
            Err(e) => {
                error!("Failed to decode audio file {hash}: {e}");
                return None;
            }
        };

        Some((samples, desc))
    }

    const MAX_FILES: usize = 64;
    fn truncate(&self) {
        let mut cache = self.cache.write();
        while cache.len() > Self::MAX_FILES {
            cache.pop_front();
        }
    }
}
