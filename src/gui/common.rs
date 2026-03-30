use std::fs::File;

use eframe::egui::RichText;
use eframe::egui::{self};
use image::{DynamicImage, GenericImage};
use log::{error, info, warn};
use quicktag_core::tagtypes::TagType;
use std::io::{Cursor, Write};
use tiger_pkg::{TagHash, package_manager};
use ww2ogg::{CodebookLibrary, WwiseRiffVorbis};

use crate::texture::{Texture, cache::TextureCache};

use super::TOASTS;

pub trait ResponseExt {
    fn tag_context(self, tag: TagHash) -> Self;

    fn tag_context_with_preview(
        self,
        tag: TagHash,
        texture_cache: Option<&TextureCache>,
        is_texture: bool,
    ) -> Self;

    fn string_context(self, s: &str, hash: Option<u32>) -> Self;
}

impl ResponseExt for egui::Response {
    fn tag_context(self, tag: TagHash) -> Self {
        let s = self.on_hover_ui(|ui| tag_hover_ui(ui, tag));
        s.context_menu(|ui| tag_context(ui, tag));
        s
    }

    fn tag_context_with_preview(
        self,
        tag: TagHash,
        texture_cache: Option<&TextureCache>,
        is_texture: bool,
    ) -> Self {
        self.context_menu(|ui| {
            if is_texture && let Some(texture_cache) = texture_cache {
                if ui.selectable_label(false, "📷 Copy texture").clicked() {
                    match Texture::load(&texture_cache.render_state, tag, false) {
                        Ok(o) => {
                            let image = o.to_image(&texture_cache.render_state, 0).unwrap();
                            let rgba = image.to_rgba8().to_vec();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                &rgba,
                            );

                            self.ctx.copy_image(color_image);
                        }
                        Err(e) => {
                            error!("Failed to load texture: {e}");
                        }
                    }
                    ui.close();
                }

                if ui
                    .selectable_label(false, "📷 Save texture")
                    .on_hover_text("Texture(s) will be saved to the textures/ directory")
                    .clicked()
                {
                    export_texture(texture_cache, tag);
                    ui.close();
                }
            }
            tag_context(ui, tag);
        });
        self.on_hover_ui(|ui| {
            if is_texture && let Some(texture_cache) = texture_cache {
                texture_cache.texture_preview(tag, ui);
            }

            tag_hover_ui(ui, tag);
        })
    }

    fn string_context(self, string: &str, hash: Option<u32>) -> Self {
        self.context_menu(|ui| {
            if ui.selectable_label(false, "Copy string").clicked() {
                ui.ctx().copy_text(string.to_string());
                ui.close();
            }
            if let Some(hash) = hash
                && ui.selectable_label(false, "Copy hash").clicked()
            {
                ui.ctx().copy_text(format!("{:08X}", hash));
                ui.close();
            }
        });

        self
    }
}

pub fn export_texture(texture_cache: &TextureCache, tag: TagHash) {
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
}

fn tag_hover_ui(ui: &mut egui::Ui, tag: TagHash) {
    if let Some(path) = package_manager().package_paths.get(&tag.pkg_id()) {
        ui.label(format!("Package: {}", path.filename));
    }

    // Render the audio playback state
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
        ui.ctx().copy_text(if copy_flipped {
            format!("{:08X}", tag.0.swap_bytes())
        } else {
            format!("{:08X}", tag.0)
        });
        ui.close();
    }

    if let Some(tag64) = package_manager().get_tag64_for_tag32(tag) {
        if ui
            .selectable_label(false, format!("📋 Copy 64-bit tag{flipped_postfix}"))
            .clicked()
        {
            ui.ctx().copy_text(if copy_flipped {
                format!("{:016X}", tag64.0.swap_bytes())
            } else {
                format!("{:016X}", tag64.0)
            });
            ui.close();
        }
    }

    if let Some(entry) = package_manager().get_entry(tag) {
        if ui
            .selectable_label(false, format!("📋 Copy reference tag{flipped_postfix}"))
            .clicked()
        {
            ui.ctx().copy_text(if copy_flipped {
                format!("{:08X}", entry.reference.swap_bytes())
            } else {
                format!("{:08X}", entry.reference)
            });
            ui.close();
        }

        let tt = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
        if tt == TagType::WwiseStream && ui.selectable_label(false, "🎵 Play audio").clicked() {
            open_audio_file_in_default_application(tag);
            ui.close();
        }
    }

    if ui
        .add_enabled(
            false,
            egui::Button::selectable(false, "📤 Open in Alkahest"),
        )
        .clicked()
    {
        warn!("Alkahest IPC not implemented yet");
        ui.close();
    }

    if ui.selectable_label(false, "📤 Open tag data").clicked() {
        open_tag_in_default_application(tag);
        ui.close();
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

pub fn open_audio_file_in_default_application(tag: TagHash) {
    std::thread::spawn(move || {
        let data = package_manager().read_tag(tag).unwrap();

        let input = Cursor::new(data);
        let mut converter =
            match WwiseRiffVorbis::new(input, CodebookLibrary::aotuv_codebooks().unwrap()) {
                Ok(o) => o,
                Err(e) => {
                    error!("Failed to decode audio file: {e}");
                    return;
                }
            };

        let path = std::env::temp_dir().join(format!("{tag}.ogg"));
        if let Ok(f) = File::create(&path) {
            converter.generate_ogg(f);
        }
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
