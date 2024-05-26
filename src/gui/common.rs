use std::fs::File;

use destiny_pkg::{TagHash, TagHash64};
use eframe::egui;
use image::ImageFormat;
use lazy_static::lazy_static;
use log::{error, info, warn};
use std::io::{Cursor, Write};
use std::num::NonZeroU32;

use crate::{packages::package_manager, tagtypes::TagType};

use super::texture::{Texture, TextureCache};

lazy_static! {
    static ref CF_PNG: NonZeroU32 = clipboard_win::register_format("PNG").unwrap();
    static ref CF_FILENAME: NonZeroU32 = clipboard_win::register_format("FileNameW").unwrap();
}

pub trait ResponseExt {
    fn tag_context(self, tag: TagHash, tag64: Option<TagHash64>) -> Self;

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        tag64: Option<TagHash64>,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self;
}

impl ResponseExt for egui::Response {
    fn tag_context(self, tag: TagHash, tag64: Option<TagHash64>) -> Self {
        let s = self.on_hover_ui(|ui| tag_hover_ui(ui, tag));
        s.context_menu(|ui| tag_context(ui, tag, tag64));
        s
    }

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        tag64: Option<TagHash64>,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self {
        self.context_menu(|ui| {
            if is_texture {
                if ui.selectable_label(false, "📷 Copy texture").clicked() {
                    match Texture::load(&texture_cache.render_state, tag, false) {
                        Ok(o) => {
                            let image = o.to_image(&texture_cache.render_state).unwrap();
                            let mut png_data = vec![];
                            let mut png_writer = Cursor::new(&mut png_data);
                            image.write_to(&mut png_writer, ImageFormat::Png).unwrap();

                            let _clipboard = clipboard_win::Clipboard::new();
                            if let Err(e) = clipboard_win::raw::set(CF_PNG.get(), &png_data) {
                                error!("Failed to copy texture to clipboard: {e}");
                            }

                            // Save to temp
                            let path = std::env::temp_dir().join(format!("{tag}.png"));
                            let mut file = File::create(&path).unwrap();
                            file.write_all(&png_data).unwrap();

                            let mut path_utf16 =
                                path.to_string_lossy().encode_utf16().collect::<Vec<u16>>();
                            path_utf16.push(0);

                            if let Err(e) = clipboard_win::raw::set_without_clear(
                                CF_FILENAME.get(),
                                bytemuck::cast_slice(&path_utf16),
                            ) {
                                error!("Failed to copy texture path to clipboard: {e}");
                            }
                        }
                        Err(e) => {
                            error!("Failed to load texture: {e}");
                        }
                    }
                    ui.close_menu();
                }
            }
            tag_context(ui, tag, tag64);
        });
        self.on_hover_ui(|ui| {
            if is_texture {
                texture_cache.texture_preview(tag, ui);
            }

            tag_hover_ui(ui, tag);
        })
    }
}

fn tag_hover_ui(ui: &mut egui::Ui, tag: TagHash) {
    if let Some(path) = package_manager().package_paths.get(&tag.pkg_id()) {
        ui.label(format!("Package: {}", path.filename));
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
