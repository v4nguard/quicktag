use destiny_pkg::TagHash;
use eframe::egui::{self, pos2, vec2, Color32, RichText};

use crate::{packages::package_manager, tagtypes::TagType};

use super::{
    common::{dump_wwise_info, ResponseExt},
    tag::format_tag_entry,
    texture::TextureCache,
    View, ViewAction,
};

pub struct TexturesView {
    selected_package: u16,
    packages_with_textures: Vec<u16>,
    texture_cache: TextureCache,
    textures: Vec<(usize, TagHash, TagType)>,
}

impl TexturesView {
    pub fn new(texture_cache: TextureCache) -> Self {
        let packages_with_textures = package_manager()
            .package_paths
            .iter()
            .filter_map(|(id, _path)| {
                for e in &package_manager().package_entry_index[id] {
                    let st = TagType::from_type_subtype(e.file_type, e.file_subtype);
                    if st.is_texture() && st.is_header() {
                        return Some(*id);
                    }
                }
                None
            })
            .collect();

        Self {
            selected_package: u16::MAX,
            packages_with_textures,
            texture_cache,
            textures: vec![],
        }
    }
}

impl View for TexturesView {
    fn view(
        &mut self,
        _ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<super::ViewAction> {
        egui::SidePanel::left("textures_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap = Some(false);
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

        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        ui.style_mut().wrap = Some(true);

                        if self.selected_package == u16::MAX {
                            ui.label(RichText::new("No package selected").italics());
                        } else {
                            ui.horizontal_wrapped(|ui| {
                                for (i, hash, tag_type) in &self.textures {
                                    let entry = package_manager().get_entry(*hash).unwrap();
                                    let entry_name = format_tag_entry(*hash, Some(&entry));

                                    let (_, img_rect) = ui.allocate_space(vec2(128.0, 128.0));
                                    if ui.is_rect_visible(img_rect) {
                                        if let Some((_tex, tid)) =
                                            self.texture_cache.get_or_load(*hash)
                                        {
                                            ui.painter_at(img_rect).image(
                                                tid,
                                                img_rect,
                                                egui::Rect::from_min_size(
                                                    pos2(0.0, 0.0),
                                                    vec2(1.0, 1.0),
                                                ),
                                                Color32::WHITE,
                                            );
                                        }
                                    }
                                }
                            });
                        }

                        None
                    })
                    .inner
            })
            .inner
    }
}
