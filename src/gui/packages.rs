use destiny_pkg::package::UEntryHeader;
use destiny_pkg::{manager::PackagePath, TagHash};
use eframe::egui::{self, RichText};

use crate::util::format_file_size;
use crate::{packages::package_manager, tagtypes::TagType};

use super::{
    common::{dump_wwise_info, ResponseExt},
    tag::format_tag_entry,
    texture::TextureCache,
    View, ViewAction,
};

pub struct PackagesView {
    selected_package: u16,
    package_entry_search_cache: Vec<(String, TagType, UEntryHeader)>,
    package_filter: String,
    package_entry_filter: String,
    texture_cache: TextureCache,
    sorted_package_paths: Vec<(u16, PackagePath)>,
}

impl PackagesView {
    pub fn new(texture_cache: TextureCache) -> Self {
        let mut sorted_package_paths: Vec<(u16, PackagePath)> = package_manager()
            .package_paths
            .iter()
            .map(|(id, path)| (*id, path.clone()))
            .collect();

        sorted_package_paths.sort_by_cached_key(|(_, path)| format!("{}_{}", path.name, path.id));

        Self {
            selected_package: u16::MAX,
            package_entry_search_cache: vec![],
            package_filter: String::new(),
            package_entry_filter: String::new(),
            texture_cache,
            sorted_package_paths,
        }
    }
}

impl View for PackagesView {
    fn view(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<super::ViewAction> {
        egui::SidePanel::left("packages_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap = Some(false);
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.package_filter);
                });
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        for (id, path) in self.sorted_package_paths.iter() {
                            let package_name = format!("{}_{}", path.name, path.id);
                            if !self.package_filter.is_empty()
                                && !package_name
                                    .to_lowercase()
                                    .contains(&self.package_filter.to_lowercase())
                            {
                                continue;
                            }

                            if ui
                                .selectable_value(
                                    &mut self.selected_package,
                                    *id,
                                    format!("{id:04x}: {package_name}"),
                                )
                                .changed()
                            {
                                self.package_entry_search_cache = vec![];
                                if let Ok(p) = package_manager().version.open(&path.path) {
                                    for (i, e) in p.entries().iter().enumerate() {
                                        let label =
                                            format_tag_entry(TagHash::new(*id, i as u16), Some(e));

                                        self.package_entry_search_cache.push((
                                            label,
                                            TagType::from_type_subtype(e.file_type, e.file_subtype),
                                            e.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    });
            });

        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.package_entry_filter);
                });
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        ui.style_mut().wrap = Some(false);

                        if self.selected_package == u16::MAX {
                            ui.label(RichText::new("No package selected").italics());
                        } else {
                            if ui.button("Export audio info").clicked() {
                                dump_wwise_info(self.selected_package);
                            }

                            for (i, (label, tag_type, entry)) in self
                                .package_entry_search_cache
                                .iter()
                                .enumerate()
                                .filter(|(_, (label, _, _))| {
                                    self.package_entry_filter.is_empty()
                                        || label.to_lowercase().contains(&self.package_entry_filter)
                                })
                            {
                                let tag = TagHash::new(self.selected_package, i as u16);
                                ctx.style_mut(|s| {
                                    s.interaction.show_tooltips_only_when_still = false;
                                    s.interaction.tooltip_delay = 0.0;
                                });
                                if ui
                                    .add(egui::SelectableLabel::new(
                                        false,
                                        RichText::new(format!(
                                            "{i}: {label} ({})",
                                            format_file_size(entry.file_size as usize)
                                        ))
                                        .color(tag_type.display_color()),
                                    ))
                                    .tag_context_with_texture(
                                        tag,
                                        &self.texture_cache,
                                        tag_type.is_texture() && tag_type.is_header(),
                                    )
                                    .clicked()
                                {
                                    return Some(ViewAction::OpenTag(tag));
                                }
                            }
                        }

                        None
                    })
                    .inner
            })
            .inner
    }
}
