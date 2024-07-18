use crate::gui::{audio, View, ViewAction};
use crate::package_manager::package_manager;
use destiny_pkg::manager::PackagePath;
use destiny_pkg::TagHash;
use eframe::egui;
use eframe::wgpu::naga::FastIndexMap;
use egui_extras::{Column, TableBuilder};

struct PackageAudio {
    pub streams: Vec<(TagHash, f32)>,
    pub events: Vec<TagHash>,
}

impl PackageAudio {
    pub fn by_pkg_id(id: u16) -> Self {
        Self {
            // TODO(cohae): This only works for game versions after beyond light
            events: package_manager()
                .get_all_by_reference(0x80809738)
                .iter()
                .filter(|(t, _)| t.pkg_id() == id)
                .map(|(t, _)| *t)
                .collect(),
            streams: package_manager()
                .get_all_by_type(26, Some(7))
                .iter()
                .filter(|(t, _)| t.pkg_id() == id)
                .map(|(t, _)| (*t, audio::get_stream_duration_fast(*t)))
                .collect(),
        }
    }
}

struct PackageAudioTypes {
    pub streams: bool,
    pub events: bool,
}

impl PackageAudioTypes {
    pub fn by_pkg_id(id: u16) -> Self {
        Self {
            // TODO(cohae): This only works for game versions after beyond light
            events: package_manager()
                .get_all_by_reference(0x80809738)
                .iter()
                .any(|(t, _)| t.pkg_id() == id),
            streams: package_manager()
                .get_all_by_type(26, Some(7))
                .iter()
                .any(|(t, _)| t.pkg_id() == id),
        }
    }
}

pub struct AudioView {
    selected_package: u16,
    selected_audio: Option<PackageAudio>,
    packages: FastIndexMap<u16, (PackagePath, PackageAudioTypes)>,
}

impl AudioView {
    pub fn new() -> Self {
        let mut sorted_package_paths: Vec<(u16, (PackagePath, PackageAudioTypes))> =
            package_manager()
                .package_paths
                .iter()
                .map(|(id, path)| (*id, (path.clone(), PackageAudioTypes::by_pkg_id(*id))))
                .collect();

        sorted_package_paths.retain(|(_, (_, p))| !p.streams || !p.events);

        sorted_package_paths
            .sort_by_cached_key(|(_, (path, _))| format!("{}_{}", path.name, path.id));

        Self {
            selected_package: u16::MAX,
            selected_audio: None,
            packages: sorted_package_paths.into_iter().collect(),
        }
    }
}

impl View for AudioView {
    fn view(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) -> Option<ViewAction> {
        egui::SidePanel::left("packages_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap = Some(false);
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        for (id, (path, pkg)) in self.packages.iter() {
                            if !pkg.streams {
                                continue;
                            }

                            let package_name = format!("{}_{}", path.name, path.id);
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
                                self.selected_package = *id;
                                self.selected_audio = Some(PackageAudio::by_pkg_id(*id));
                            }
                        }
                    });
            });

        if let Some(audio) = &self.selected_audio {
            let text_height = egui::TextStyle::Body
                .resolve(ui.style())
                .size
                .max(ui.spacing().interact_size.y);

            let table = TableBuilder::new(ui)
                .column(Column::auto().at_least(36.0))
                .column(Column::auto().at_least(96.0))
                .column(Column::remainder());

            table
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("#");
                    });
                    header.col(|ui| {
                        ui.strong("Tag");
                    });
                    header.col(|ui| {
                        ui.strong("Duration");
                    });
                })
                .body(|mut body| {
                    body.rows(text_height, audio.streams.len(), |mut row| {
                        let (tag, duration) = &audio.streams[row.index()];
                        row.col(|ui| {
                            ui.label(tag.entry_index().to_string());
                        });
                        row.col(|ui| {
                            ui.label(tag.to_string());
                        });
                        row.col(|ui| {
                            ui.label(format_duration(*duration));
                        });
                    })
                });
        }

        None
    }
}

fn format_duration(d: f32) -> String {
    if d >= 60.0 {
        format!("{}:{:.1}", (d / 60.0).floor(), d % 60.0)
    } else {
        format!("{d:.1}")
    }
}
