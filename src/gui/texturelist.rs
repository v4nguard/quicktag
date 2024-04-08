use destiny_pkg::{manager::PackagePath, TagHash};
use eframe::egui::{self, pos2, vec2, Color32, RichText, Stroke, Widget};

use crate::{packages::package_manager, tagtypes::TagType};

use super::{common::ResponseExt, texture::TextureCache, View, ViewAction};

pub struct TexturesView {
    selected_package: u16,
    packages_with_textures: Vec<u16>,
    package_filter: String,
    texture_cache: TextureCache,
    textures: Vec<(usize, TagHash, TagType)>,

    keep_aspect_ratio: bool,
    zoom: f32,
}

impl TexturesView {
    pub fn new(texture_cache: TextureCache) -> Self {
        Self {
            selected_package: u16::MAX,
            packages_with_textures: Self::search_textures(None),
            package_filter: String::new(),
            texture_cache,
            textures: vec![],
            keep_aspect_ratio: true,
            zoom: 1.0,
        }
    }

    fn search_textures(search: Option<String>) -> Vec<u16> {
        let mut packages: Vec<(u16, PackagePath)> = package_manager()
            .package_paths
            .iter()
            .filter_map(|(id, path)| {
                if let Some(entries) = package_manager().package_entry_index.get(id) {
                    for e in entries {
                        let st = TagType::from_type_subtype(e.file_type, e.file_subtype);
                        if st.is_texture() && st.is_header() {
                            return Some((*id, path.clone()));
                        }
                    }
                }

                None
            })
            .collect();

        if let Some(search) = search {
            let search = search.to_lowercase();
            packages.retain(|(_id, path)| {
                format!("{}_{}", path.name, path.id)
                    .to_lowercase()
                    .contains(&search)
            });
        }

        packages.sort_by_cached_key(|(_id, path)| format!("{}_{}", path.name, path.id));

        packages.into_iter().map(|(id, _path)| id).collect()
    }
}

impl View for TexturesView {
    fn view(
        &mut self,
        _ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<super::ViewAction> {
        let mut action = None;
        egui::SidePanel::left("textures_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap = Some(false);

                ui.horizontal(|ui| {
                    ui.label("Search:");
                    if ui.text_edit_singleline(&mut self.package_filter).changed() {
                        self.packages_with_textures =
                            Self::search_textures(if self.package_filter.is_empty() {
                                None
                            } else {
                                Some(self.package_filter.clone())
                            });
                    }
                });
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        for id in &self.packages_with_textures {
                            let path = &package_manager().package_paths[id];
                            let package_name = format!("{}_{}", path.name, path.id);

                            if ui
                                .selectable_value(
                                    &mut self.selected_package,
                                    *id,
                                    format!("{id:04x}: {package_name}"),
                                )
                                .changed()
                            {
                                self.textures = package_manager().package_entry_index[id]
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, e)| {
                                        let st =
                                            TagType::from_type_subtype(e.file_type, e.file_subtype);
                                        if st.is_texture() && st.is_header() {
                                            Some((i, TagHash::new(*id, i as u16), st))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                            }
                        }
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Zoom: ");
                egui::Slider::new(&mut self.zoom, 0.25..=2.0)
                    .show_value(true)
                    .ui(ui);

                ui.checkbox(&mut self.keep_aspect_ratio, "Keep aspect ratio");
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_width(f32::INFINITY)
                .show(ui, |ui| {
                    ui.style_mut().wrap = Some(true);
                    ui.spacing_mut().item_spacing = [4. * self.zoom; 2].into();

                    if self.selected_package == u16::MAX {
                        ui.label(RichText::new("No package selected").italics());
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.ctx().style_mut(|s| {
                                s.interaction.show_tooltips_only_when_still = false;
                                s.interaction.tooltip_delay = 0.0;
                            });

                            for (_i, hash, _tag_type) in &self.textures {
                                let img_container = ui.allocate_response(
                                    vec2(128.0 * self.zoom, 128.0 * self.zoom),
                                    egui::Sense::click(),
                                );

                                let img_container_rect = img_container.rect;

                                if ui.is_rect_visible(img_container_rect) {
                                    let (tex, tid) = self.texture_cache.get_or_default(*hash);
                                    // The rect of the actual image itself, with aspect ratio corrections applied
                                    let img_rect = if self.keep_aspect_ratio {
                                        if tex.width > tex.height {
                                            let scale =
                                                img_container_rect.width() / tex.width as f32;
                                            let height = tex.height as f32 * scale;
                                            let y = img_container_rect.center().y - height / 2.0;
                                            egui::Rect::from_min_size(
                                                pos2(img_container_rect.left(), y),
                                                vec2(img_container_rect.width(), height),
                                            )
                                        } else {
                                            let scale =
                                                img_container_rect.height() / tex.height as f32;
                                            let width = tex.width as f32 * scale;
                                            let x = img_container_rect.center().x - width / 2.0;
                                            egui::Rect::from_min_size(
                                                pos2(x, img_container_rect.top()),
                                                vec2(width, img_container_rect.height()),
                                            )
                                        }
                                    } else {
                                        img_container_rect
                                    };

                                    let painter = ui.painter_at(img_container_rect);

                                    painter.rect_filled(img_container_rect, 4.0, Color32::BLACK);
                                    painter.image(
                                        tid,
                                        img_rect,
                                        egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );

                                    if img_container.hovered() {
                                        ui.painter().rect_stroke(
                                            img_container_rect,
                                            4.0,
                                            Stroke::new(1.0, Color32::WHITE),
                                        );
                                    }

                                    if img_container
                                        .tag_context_with_texture(
                                            *hash,
                                            None,
                                            &self.texture_cache,
                                            true,
                                        )
                                        .clicked()
                                    {
                                        action = Some(ViewAction::OpenTag(*hash));
                                    }
                                }
                            }
                        });
                    }
                });
        });

        action
    }
}
