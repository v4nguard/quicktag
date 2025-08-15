use std::fs::File;

use eframe::egui;
use eframe::egui::RichText;
use image::{DynamicImage, GenericImage, ImageFormat};
use lazy_static::lazy_static;
use log::{error, info, warn};
use quicktag_core::tagtypes::TagType;
use std::io::{Cursor, Write};
use std::num::NonZeroU32;
use tiger_pkg::{TagHash, package_manager};

use crate::texture::{Texture, cache::TextureCache};

use super::TOASTS;

lazy_static! {
    static ref CF_PNG: NonZeroU32 = clipboard_win::register_format("PNG").unwrap();
    static ref CF_FILENAME: NonZeroU32 = clipboard_win::register_format("FileNameW").unwrap();
}

pub trait ResponseExt {
    fn tag_context(self, tag: TagHash) -> Self;

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self;
}

impl ResponseExt for egui::Response {
    fn tag_context(self, tag: TagHash) -> Self {
        let s = self.on_hover_ui(|ui| tag_hover_ui(ui, tag));
        s.context_menu(|ui| tag_context(ui, tag));
        s
    }

    fn tag_context_with_texture(
        self,
        tag: TagHash,
        texture_cache: &TextureCache,
        is_texture: bool,
    ) -> Self {
        self.context_menu(|ui| {
            if is_texture {
                if ui.selectable_label(false, "📷 Copy texture").clicked() {
                    match Texture::load(&texture_cache.render_state, tag, false) {
                        Ok(o) => {
                            let image = o.to_image(&texture_cache.render_state, 0).unwrap();
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
                            } else {
                                TOASTS.lock().success("Texture copied to clipboard");
                            }
                        }
                        Err(e) => {
                            error!("Failed to load texture: {e}");
                        }
                    }
                    ui.close_menu();
                }

                if ui
                    .selectable_label(false, "📷 Save texture")
                    .on_hover_text("Texture(s) will be saved to the textures/ directory")
                    .clicked()
                {
                    match Texture::load(&texture_cache.render_state, tag, false) {
                        Ok(o) => {
                            std::fs::create_dir_all("textures/").unwrap();
                            let mut images = vec![];
                            for layer in 0..(o.desc.array_size.max(o.desc.depth)) {
                                let image = o.to_image(&texture_cache.render_state, layer).unwrap();
                                image.save(format!("textures/{tag}_{layer}.png")).unwrap();
                                images.push(image);
                            }

                            if images.len() == 6 {
                                let cubemap_image = assemble_cubemap(images);
                                cubemap_image
                                    .save(format!("textures/{tag}_cubemap.png"))
                                    .unwrap();
                            }
                            TOASTS.lock().success("Texture saved");
                        }
                        Err(e) => {
                            error!("Failed to load texture: {e}");
                        }
                    }
                    ui.close_menu();
                }
            }
            tag_context(ui, tag);
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

    // Render the audio playback state
    #[cfg(feature = "audio")]
    if let Some(entry) = package_manager().get_entry(tag) {
        use crate::gui::audio::{AudioPlayer, AudioPlayerState};
        let tag_type = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
        if tag_type == TagType::WwiseStream {
            let state = AudioPlayer::instance().play(tag);

            match state {
                AudioPlayerState::Loading => {
                    AudioPlayer::instance().stop();
                    ui.label("Loading audio for playback...");
                }
                AudioPlayerState::Errored(e) => {
                    AudioPlayer::instance().stop();
                    ui.label(RichText::new(e).color(egui::Color32::RED));
                }
                AudioPlayerState::Playing(p) => {
                    ui.separator();

                    let current_time = p.time.elapsed().as_secs_f32().min(p.duration);
                    ui.label(format!(
                        "{}/{}",
                        format_time(current_time),
                        format_time(p.duration)
                    ));
                    let (playbar_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_size_before_wrap().x * 0.6, 8.0),
                        egui::Sense::hover(),
                    );
                    let playback_pos = current_time / p.duration;
                    let playbar_rect_fill = egui::Rect {
                        max: egui::pos2(
                            playbar_rect.min.x + playbar_rect.width() * playback_pos,
                            playbar_rect.max.y,
                        ),
                        ..playbar_rect
                    };

                    let pbpaint = ui.painter_at(playbar_rect);
                    pbpaint.rect_filled(
                        playbar_rect,
                        16.0,
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 16),
                    );
                    pbpaint.rect_filled(
                        playbar_rect_fill,
                        16.0,
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 64),
                    );
                    ui.painter().circle_filled(
                        playbar_rect_fill.right_center() - egui::vec2(4.0, 0.0),
                        5.0,
                        egui::Color32::WHITE,
                    );
                }
            }

            // Refresh the UI every frame to repaint the playbar
            ui.ctx().request_repaint();
        } else {
            AudioPlayer::instance().stop();
        }
    }
}

/// Formats seconds into m:ss
fn format_time(seconds: f32) -> String {
    let minutes = seconds as usize / 60;
    let seconds = seconds as usize % 60;

    format!("{minutes}:{seconds:02}")
}

pub fn tag_context(ui: &mut egui::Ui, tag: TagHash) {
    let copy_flipped = ui.input(|i| i.modifiers.shift);
    let flipped_postfix = if copy_flipped { " - old style" } else { "" };
    if ui
        .selectable_label(false, format!("📋 Copy tag{flipped_postfix}"))
        .clicked()
    {
        ui.output_mut(|o| {
            o.copied_text = if copy_flipped {
                format!("{:08X}", tag.0.swap_bytes())
            } else {
                format!("{:08X}", tag.0)
            }
        });
        ui.close_menu();
    }

    if let Some(tag64) = package_manager().get_tag64_for_tag32(tag) {
        if ui
            .selectable_label(false, format!("📋 Copy 64-bit tag{flipped_postfix}"))
            .clicked()
        {
            ui.output_mut(|o| {
                o.copied_text = if copy_flipped {
                    format!("{:016X}", tag64.0.swap_bytes())
                } else {
                    format!("{:016X}", tag64.0)
                }
            });
            ui.close_menu();
        }
    }

    if let Some(entry) = package_manager().get_entry(tag) {
        if ui
            .selectable_label(false, format!("📋 Copy reference tag{flipped_postfix}"))
            .clicked()
        {
            ui.output_mut(|o| {
                o.copied_text = if copy_flipped {
                    format!("{:08X}", entry.reference.swap_bytes())
                } else {
                    format!("{:08X}", entry.reference)
                }
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

    if ui.selectable_label(false, "📤 Open tag data").clicked() {
        open_tag_in_default_application(tag);
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

#[cfg(not(feature = "audio"))]
pub fn open_audio_file_in_default_application(_tag: TagHash, _ext: &str) {}

#[cfg(feature = "audio")]
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
            // TODO(cohae): Replace with `hound` crate
            #[allow(deprecated)]
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

#[cfg(not(feature = "audio"))]
pub fn dump_wwise_info(_package_id: u16) {}

#[cfg(feature = "audio")]
pub fn dump_wwise_info(package_id: u16) {
    use tiger_pkg::Version;

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

#[allow(clippy::erasing_op)]
fn assemble_cubemap(images: Vec<DynamicImage>) -> DynamicImage {
    let tile_w = images[0].width();
    let tile_h = images[0].height();

    let mut cubemap = DynamicImage::new_rgba8(tile_w * 4, tile_h * 3);

    let x_pos = images[0].rotate90();
    let x_neg = images[1].rotate270();
    let y_pos = images[2].rotate180();
    let y_neg = images[3].clone();
    let z_pos = images[4].rotate90();
    let z_neg = images[5].rotate90();

    // -- Z+ -- --
    // Y- X+ Y+ X-
    // -- Z- -- --
    let _ = cubemap.copy_from(&z_pos, tile_w, tile_h * 0);
    let _ = cubemap.copy_from(&y_neg, tile_w * 0, tile_h);
    let _ = cubemap.copy_from(&x_pos, tile_w, tile_h);
    let _ = cubemap.copy_from(&y_pos, tile_w * 2, tile_h);
    let _ = cubemap.copy_from(&x_neg, tile_w * 3, tile_h);
    let _ = cubemap.copy_from(&z_neg, tile_w, tile_h * 2);

    cubemap
}
