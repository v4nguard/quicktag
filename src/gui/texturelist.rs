use destiny_pkg::{manager::PackagePath, TagHash};
use eframe::egui::{self, pos2, vec2, Color32, Pos2, RichText, Stroke, Ui, Vec2, Widget};
use eframe::emath::Rot2;
use std::fmt::{Display, Formatter};

use crate::gui::texture::{Texture, TextureDesc};
use crate::util::ui_image_rotated;
use crate::{package_manager::package_manager, tagtypes::TagType};

use super::{common::ResponseExt, texture::TextureCache, View, ViewAction};

pub struct TexturesView {
    selected_package: u16,
    packages_with_textures: Vec<u16>,
    package_filter: String,
    texture_cache: TextureCache,
    textures: Vec<(usize, TagHash, TagType, Option<TextureDesc>)>,

    keep_aspect_ratio: bool,
    zoom: f32,
    sorting: Sorting,
    filter_texdesc: String,
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
            sorting: Sorting::IndexAsc,
            filter_texdesc: String::new(),
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

    fn apply_sorting(&mut self) {
        match self.sorting {
            Sorting::IndexAsc | Sorting::IndexDesc => {
                self.textures.sort_by_cached_key(|(i, _, _, _)| *i);
            }
            Sorting::SizeAsc | Sorting::SizeDesc => {
                self.textures.sort_by_cached_key(|(_, _, _, desc)| {
                    desc.as_ref().map(|d| d.width * d.height).unwrap_or(0)
                });
            }
        }

        if self.sorting.is_descending() {
            self.textures.reverse();
        }
    }
}

impl View for TexturesView {
    fn view(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) -> Option<ViewAction> {
        let mut action = None;
        egui::SidePanel::left("textures_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

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
                        let mut update_filters = false;
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

                                        let hash = TagHash::new(*id, i as u16);
                                        if st.is_texture() && st.is_header() {
                                            Some((i, hash, st, Texture::load_desc(hash).ok()))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();

                                update_filters = true;
                            }
                        }

                        // cohae: Activate at your own risk. May cause death
                        // if ui
                        //     .selectable_value(
                        //         &mut self.selected_package,
                        //         0xffff - 1,
                        //         "All Uncompressed".to_string(),
                        //     )
                        //     .changed()
                        // {
                        //     self.textures = package_manager()
                        //         .package_entry_index
                        //         .iter()
                        //         .par_bridge()
                        //         .flat_map(|(pkg_id, entries)| {
                        //             let mut tex_entries = vec![];
                        //             for (i, e) in entries.iter().enumerate() {
                        //                 let hash = TagHash::new(*pkg_id, i as u16);
                        //                 let st =
                        //                     TagType::from_type_subtype(e.file_type, e.file_subtype);
                        //                 if st.is_texture() && st.is_header() {
                        //                     let Ok(desc) = Texture::load_desc(hash) else {
                        //                         continue;
                        //                     };
                        //
                        //                     if !desc.format.is_compressed() {
                        //                         tex_entries.push((
                        //                             (*pkg_id as usize) * 8192 + i,
                        //                             hash,
                        //                             st,
                        //                             Some(desc),
                        //                         ));
                        //                     }
                        //                 }
                        //             }
                        //
                        //             tex_entries
                        //         })
                        //         .collect();
                        //
                        //     update_filters = true;
                        // }

                        if update_filters {
                            self.apply_sorting();
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

                #[allow(clippy::blocks_in_conditions)]
                if egui::ComboBox::from_label("Sort by")
                    .selected_text(self.sorting.to_string())
                    .show_ui(ui, |ui| {
                        let mut changed = ui
                            .selectable_value(&mut self.sorting, Sorting::IndexAsc, "Index ⬆")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.sorting, Sorting::IndexDesc, "Index ⬇")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.sorting, Sorting::SizeAsc, "Size ⬆")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.sorting, Sorting::SizeDesc, "Size ⬇")
                            .changed();
                        changed
                    })
                    .inner
                    .unwrap_or(false)
                {
                    self.apply_sorting();
                }
            });

            ui.horizontal(|ui| {
                ui.label("Texture desc filter: ");
                ui.text_edit_singleline(&mut self.filter_texdesc).changed();
            });

            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_width(f32::INFINITY)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.spacing_mut().item_spacing = [4. * self.zoom; 2].into();

                    if self.selected_package == u16::MAX {
                        ui.label(RichText::new("No package selected").italics());
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.ctx().style_mut(|s| {
                                s.interaction.show_tooltips_only_when_still = false;
                                s.interaction.tooltip_delay = 0.0;
                            });

                            let filter = self.filter_texdesc.to_lowercase();
                            for (_i, hash, _tag_type, desc) in &self.textures {
                                if let Some(desc) = desc {
                                    if !filter.is_empty()
                                        && !desc.info().to_lowercase().contains(&filter)
                                    {
                                        continue;
                                    }
                                }

                                let img_container = ui.allocate_response(
                                    vec2(128.0 * self.zoom, 128.0 * self.zoom),
                                    egui::Sense::click(),
                                );

                                let img_container_rect = img_container.rect;

                                if ui.is_rect_visible(img_container_rect) {
                                    let (tex, tid) = self.texture_cache.get_or_default(*hash);
                                    // The rect of the actual image itself, with aspect ratio corrections applied
                                    let img_rect = if self.keep_aspect_ratio {
                                        if tex.desc.width > tex.desc.height {
                                            let scale =
                                                img_container_rect.width() / tex.desc.width as f32;
                                            let height = tex.desc.height as f32 * scale;
                                            let y = img_container_rect.center().y - height / 2.0;
                                            egui::Rect::from_min_size(
                                                pos2(img_container_rect.left(), y),
                                                vec2(img_container_rect.width(), height),
                                            )
                                        } else {
                                            let scale = img_container_rect.height()
                                                / tex.desc.height as f32;
                                            let width = tex.desc.width as f32 * scale;
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
                                    // painter.image(
                                    //     tid,
                                    //     img_rect,
                                    //     // egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1.0, 1.0)),
                                    //     egui::Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 0.0)),
                                    //     Color32::WHITE,
                                    // );
                                    ui_image_rotated(
                                        &painter,
                                        tid,
                                        img_rect,
                                        // Rotate the image if it's a cubemap
                                        if tex.desc.array_size == 6 { 90. } else { 0. },
                                        tex.desc.array_size == 6,
                                    );

                                    if img_container.hovered() {
                                        ui.painter().rect_stroke(
                                            img_container_rect,
                                            4.0,
                                            Stroke::new(1.0, Color32::WHITE),
                                        );
                                    }

                                    if img_container
                                        .tag_context_with_texture(*hash, &self.texture_cache, true)
                                        .on_hover_text(RichText::new(format!("{hash}")).strong())
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

#[derive(Default, PartialEq)]
pub enum Sorting {
    #[default]
    IndexAsc,
    IndexDesc,

    SizeAsc,
    SizeDesc,
}

impl Sorting {
    pub fn is_descending(&self) -> bool {
        matches!(self, Sorting::IndexDesc | Sorting::SizeDesc)
    }
}

impl Display for Sorting {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Sorting::IndexAsc => f.write_str("Index ⬆"),
            Sorting::IndexDesc => f.write_str("Index ⬇"),
            Sorting::SizeAsc => f.write_str("Size ⬆"),
            Sorting::SizeDesc => f.write_str("Size ⬇"),
        }
    }
}
