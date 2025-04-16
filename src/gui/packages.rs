use eframe::egui::{self, RichText};
use quicktag_core::tagtypes::TagType;
use tiger_pkg::{manager::PackagePath, package::UEntryHeader, package_manager, TagHash, Version};

use super::{
    common::{dump_wwise_info, ResponseExt},
    tag::format_tag_entry,
    View, ViewAction,
};
use crate::gui::common::open_audio_file_in_default_application;
use crate::texture::TextureCache;
use crate::util::format_file_size;

pub struct PackagesView {
    selected_package: u16,
    package_entry_search_cache: Vec<(usize, String, TagType, UEntryHeader)>,
    package_filter: String,
    package_entry_filter: String,
    texture_cache: TextureCache,
    sorted_package_paths: Vec<(u16, PackagePath)>,
    show_only_hash64: bool,
    sort_by_size: bool,
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
            show_only_hash64: false,
            sort_by_size: false,
        }
    }

    pub fn sort_entries(&mut self) {
        if self.sort_by_size {
            self.package_entry_search_cache
                .sort_by_key(|(_, _, _, entry)| entry.file_size);
            self.package_entry_search_cache.reverse();
        } else {
            self.package_entry_search_cache
                .sort_by_key(|(i, _, _, _)| *i);
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
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.package_filter);
                });
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        let mut sort_entries = false;
                        for (id, path) in self.sorted_package_paths.iter() {
                            let package_name = format!("{}_{}", path.name, path.id);
                            if !self.package_filter.is_empty()
                                && !package_name
                                    .to_lowercase()
                                    .contains(&self.package_filter.to_lowercase())
                            {
                                continue;
                            }

                            let redacted = if path.name.ends_with("redacted") {
                                "🗝 "
                            } else {
                                ""
                            };

                            if ui
                                .selectable_value(
                                    &mut self.selected_package,
                                    *id,
                                    format!("{id:04x}: {redacted}{package_name}"),
                                )
                                .changed()
                            {
                                self.package_entry_search_cache = vec![];
                                if let Ok(p) = package_manager().version.open(&path.path) {
                                    for (i, e) in p.entries().iter().enumerate() {
                                        let label =
                                            format_tag_entry(TagHash::new(*id, i as u16), Some(e));

                                        self.package_entry_search_cache.push((
                                            i,
                                            label,
                                            TagType::from_type_subtype(e.file_type, e.file_subtype),
                                            e.clone(),
                                        ));
                                        sort_entries = true;
                                    }
                                }
                            }
                        }

                        if sort_entries {
                            self.sort_entries();
                        }
                    });
            });

        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                if self.selected_package == u16::MAX {
                    ui.label(RichText::new("No package selected").italics());

                    None
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        ui.text_edit_singleline(&mut self.package_entry_filter);
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Export audio info").clicked() {
                            dump_wwise_info(self.selected_package);
                        }

                        ui.checkbox(&mut self.show_only_hash64, "★ Only show hash64");
                        if ui
                            .checkbox(&mut self.sort_by_size, "Sort by size descending")
                            .changed()
                        {
                            self.sort_entries();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_width(f32::INFINITY)
                        .show(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

                            for (i, (tag, label, tag_type, entry)) in self
                                .package_entry_search_cache
                                .iter()
                                .enumerate()
                                .filter(|(_, (_, label, _, _))| {
                                    self.package_entry_filter.is_empty()
                                        || label
                                            .to_lowercase()
                                            .contains(&self.package_entry_filter.to_lowercase())
                                })
                                .map(|(_, (i, label, tag_type, entry))| {
                                    let tag = TagHash::new(self.selected_package, *i as u16);
                                    (i, (tag, label.clone(), tag_type, entry))
                                })
                                .filter(|(_, (tag, _, _, _))| {
                                    !self.show_only_hash64
                                        || package_manager().get_tag64_for_tag32(*tag).is_some()
                                })
                            {
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
                                    if ui.input(|i| i.modifiers.ctrl)
                                        && *tag_type == TagType::WwiseStream
                                    {
                                        open_audio_file_in_default_application(tag, "wem");
                                    } else {
                                        return Some(ViewAction::OpenTag(tag));
                                    }
                                }
                            }

                            None
                        })
                        .inner
                }
            })
            .inner
    }
}
