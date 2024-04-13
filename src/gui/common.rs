use std::fs::File;

use destiny_pkg::{TagHash, TagHash64};
use eframe::egui;
use log::{error, info, warn};
use std::io::Write;

use crate::{packages::package_manager, tagtypes::TagType};

use super::texture::TextureCache;

pub trait ResponseExt {
    fn tag_context(&self, tag: TagHash, tag64: Option<TagHash64>) -> &Self;

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        tag64: Option<TagHash64>,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self;
}

impl ResponseExt for egui::Response {
    fn tag_context(&self, tag: TagHash, tag64: Option<TagHash64>) -> &Self {
        self.context_menu(|ui| tag_context(ui, tag, tag64));
        self
    }

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        tag64: Option<TagHash64>,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self {
        self.context_menu(|ui| tag_context(ui, tag, tag64));
        if is_texture {
            self.on_hover_ui(|ui| {
                texture_cache.texture_preview(tag, ui);
            })
        } else {
            self
        }
    }
}

pub fn tag_context(ui: &mut egui::Ui, tag: TagHash, tag64: Option<TagHash64>) {
    if ui.selectable_label(false, "📋 Copy tag").clicked() {
        ui.output_mut(|o| o.copied_text = tag.to_string());
        ui.close_menu();
    }

    if let Some(tag64) = tag64 {
        if ui.selectable_label(false, "📋 Copy 64-bit tag").clicked() {
            ui.output_mut(|o| o.copied_text = tag64.to_string());
            ui.close_menu();
        }
    }

    if let Some(entry) = package_manager().get_entry(tag) {
        let shift = ui.input(|i| i.modifiers.shift);

        if ui
            .selectable_label(
                false,
                format!(
                    "📋 Copy reference tag{}",
                    if shift { " (native endian)" } else { "" }
                ),
            )
            .clicked()
        {
            ui.output_mut(|o| {
                o.copied_text = format!(
                    "{:08X}",
                    if shift {
                        entry.reference
                    } else {
                        entry.reference.to_be()
                    }
                )
            });
            ui.close_menu();
        }

        let tt = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
        if tt == TagType::WwiseStream && ui.selectable_label(false, "🎵 Play audio").clicked() {
            open_audio_file_in_default_application(tag, "wem");
            ui.close_menu();
        }
    }

    if ui
        .add_enabled(
            false,
            egui::SelectableLabel::new(false, "📤 Open in Alkahest"),
        )
        .clicked()
    {
        warn!("Alkahest IPC not implemented yet");
        ui.close_menu();
    }
}

pub fn open_tag_in_default_application(tag: TagHash) {
    let data = package_manager().read_tag(tag).unwrap();
    let entry = package_manager().get_entry(tag).unwrap();

    let filename = format!(
        "{tag}_ref-{:08X}_{}_{}.bin",
        entry.reference.to_be(),
        entry.file_type,
        entry.file_subtype,
    );

    let path = std::env::temp_dir().join(filename);
    std::fs::write(&path, data).ok();

    opener::open(path).ok();
}

pub fn open_audio_file_in_default_application(tag: TagHash, ext: &str) {
    let filename = format!(".\\{tag}.{ext}");
    std::thread::spawn(move || {
        let data = package_manager().read_tag(tag).unwrap();

        let (samples, desc) = match vgmstream::read_file_to_samples(&data, Some(filename)) {
            Ok(o) => o,
            Err(e) => {
                error!("Failed to decode audio file: {e}");
                return;
            }
        };

        let filename_wav = format!("{tag}.wav");

        let path = std::env::temp_dir().join(filename_wav);
        // std::fs::write(&path, data).ok();
        if let Ok(mut f) = File::create(&path) {
            wav::write(
                wav::Header {
                    audio_format: wav::WAV_FORMAT_PCM,
                    channel_count: desc.channels as u16,
                    sampling_rate: desc.sample_rate as u32,
                    bytes_per_second: desc.bitrate as u32,
                    bytes_per_sample: 2,
                    bits_per_sample: 16,
                },
                &wav::BitDepth::Sixteen(samples),
                &mut f,
            )
            .unwrap();

            opener::open(path).ok();
        }
    });
}

pub fn dump_wwise_info(package_id: u16) {
    let package_path = package_manager()
        .package_paths
        .get(&package_id)
        .cloned()
        .unwrap();
    let version = package_manager().version;
    std::thread::spawn(move || {
        let mut info_file = File::create(format!("wwise_info_{:04x}.txt", package_id)).unwrap();
        let package = version.open(&package_path.path).unwrap();
        let mut infos = vec![];
        for (i, _e) in package.entries().iter().enumerate().filter(|(_, e)| {
            TagType::from_type_subtype(e.file_type, e.file_subtype) == TagType::WwiseStream
        }) {
            let tag = TagHash::new(package_id, i as u16);
            if let Ok(p) = package.read_entry(i) {
                if let Ok(info) = vgmstream::read_file_info(&p, Some(format!(".\\{tag}.wem"))) {
                    infos.push((tag, info));
                }
            }
        }

        infos.sort_by_key(|(_, info)| {
            ((info.num_samples as f32 / info.sample_rate as f32) * 100.0) as usize
        });

        for (tag, info) in infos {
            writeln!(
                &mut info_file,
                "{tag} - samplerate={}hz samples={} duration={:.2}",
                info.sample_rate,
                info.num_samples,
                info.num_samples as f32 / info.sample_rate as f32
            )
            .ok();
        }

        info!("dump_wwise_info: Done");
    });
}
