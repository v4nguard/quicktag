use crate::gui::audio::AudioPlayer;
use crate::gui::common::{tag_context, ResponseExt};
use crate::gui::texturelist::Sorting;
use crate::gui::{audio, View, ViewAction};
use crate::package_manager::package_manager;
use crate::tagtypes::TagType;
use destiny_pkg::manager::PackagePath;
use destiny_pkg::{GameVersion, TagHash};
use eframe::egui;
use eframe::egui::{Key, Widget};
use eframe::wgpu::naga::FastIndexMap;
use egui_extras::{Column, TableBuilder};
use std::time::{Duration, Instant};

struct PackageAudio {
    pub streams: Vec<(TagHash, f32)>,
    pub events: Vec<TagHash>,
}

#[derive(Default, PartialEq)]
enum AudioSorting {
    #[default]
    IndexAsc,
    IndexDesc,

    DurationAsc,
    DurationDesc,
}

fn wwise_stream_type() -> (u8, u8) {
    match package_manager().version {
        GameVersion::DestinyInternalAlpha => (2, 16),
        GameVersion::DestinyTheTakenKing => (2, 21),
        GameVersion::DestinyRiseOfIron => (8, 21),
        GameVersion::Destiny2Beta
        | GameVersion::Destiny2Forsaken
        | GameVersion::Destiny2Shadowkeep => (26, 6),
        GameVersion::Destiny2BeyondLight
        | GameVersion::Destiny2WitchQueen
        | GameVersion::Destiny2Lightfall
        | GameVersion::Destiny2TheFinalShape => (26, 7),
    }
}

impl PackageAudio {
    pub fn by_pkg_id(id: u16) -> Self {
        let (wwise_type, wwise_subtype) = wwise_stream_type();
        Self {
            // TODO(cohae): Reading events only works for game versions after beyond light
            events: package_manager()
                .get_all_by_reference(0x80809738)
                .iter()
                .filter(|(t, _)| t.pkg_id() == id)
                .map(|(t, _)| *t)
                .collect(),
            streams: package_manager()
                .get_all_by_type(wwise_type, Some(wwise_subtype))
                .iter()
                .filter(|(t, e)| t.pkg_id() == id)
                .map(|(t, _)| (*t, audio::get_stream_duration_fast(*t)))
                .collect(),
        }
    }

    pub fn sort(&mut self, sort: AudioSorting) {
        self.streams
            .sort_by(|(tag_a, duration_a), (tag_b, duration_b)| match sort {
                AudioSorting::IndexAsc | AudioSorting::IndexDesc => {
                    tag_a.entry_index().cmp(&tag_b.entry_index())
                }
                AudioSorting::DurationAsc | AudioSorting::DurationDesc => {
                    duration_a.total_cmp(duration_b)
                }
            });

        if matches!(sort, AudioSorting::IndexDesc | AudioSorting::DurationDesc) {
            self.streams.reverse();
        }
    }
}

struct PackageAudioTypes {
    pub streams: bool,
    pub events: bool,
}

impl PackageAudioTypes {
    pub fn by_pkg_id(id: u16) -> Self {
        let (wwise_type, wwise_subtype) = wwise_stream_type();
        Self {
            // TODO(cohae): This only works for game versions after beyond light
            events: package_manager()
                .get_all_by_reference(0x80809738)
                .iter()
                .any(|(t, _)| t.pkg_id() == id),
            streams: package_manager()
                .get_all_by_type(wwise_type, Some(wwise_subtype))
                .iter()
                .any(|(t, _)| t.pkg_id() == id),
        }
    }
}

pub struct AudioView {
    selected_package: u16,
    selected_audio: Option<PackageAudio>,
    packages: FastIndexMap<u16, (PackagePath, PackageAudioTypes)>,
    current_row: usize,

    autoplay: bool,
    autoplay_timer: Instant,
    autoplay_interval: f32,
}

impl AudioView {
    pub fn new() -> Self {
        let mut sorted_package_paths: Vec<(u16, (PackagePath, PackageAudioTypes))> =
            package_manager()
                .package_paths
                .iter()
                .map(|(id, path)| (*id, (path.clone(), PackageAudioTypes::by_pkg_id(*id))))
                .collect();

        sorted_package_paths.retain(|(_, (_, p))| p.streams || p.events);

        sorted_package_paths
            .sort_by_cached_key(|(_, (path, _))| format!("{}_{}", path.name, path.id));

        Self {
            selected_package: u16::MAX,
            selected_audio: None,
            packages: sorted_package_paths.into_iter().collect(),
            current_row: 0,
            autoplay: false,
            autoplay_timer: Instant::now(),
            autoplay_interval: 1.0,
        }
    }
}

impl View for AudioView {
    fn view(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> Option<ViewAction> {
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
                                self.selected_audio
                                    .as_mut()
                                    .unwrap()
                                    .sort(AudioSorting::DurationAsc);
                                self.current_row = 0;
                            }
                        }
                    });
            });

        let mut row_changed = false;
        if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
            self.current_row = self.current_row.wrapping_add(1);
            row_changed = true;
        }

        if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
            self.current_row = self.current_row.wrapping_sub(1);
            row_changed = true;
        }
        if ui.input(|i| i.key_pressed(Key::PageDown)) {
            self.current_row = self.current_row.wrapping_add(10);
            row_changed = true;
        }

        if ui.input(|i| i.key_pressed(Key::PageUp)) {
            self.current_row = self.current_row.wrapping_sub(10);
            row_changed = true;
        }

        // Abort autoplay
        if self.autoplay && row_changed {
            self.autoplay = false;
        }

        if self.autoplay {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
            if self.autoplay_timer.elapsed().as_secs_f32() >= self.autoplay_interval {
                self.current_row = self.current_row.wrapping_add(1);
                row_changed = true;
                self.autoplay_timer = Instant::now();
            }
        }

        if let Some(audio) = &self.selected_audio {
            self.current_row = self.current_row.clamp(0, audio.streams.len());
            let text_height = egui::TextStyle::Body
                .resolve(ui.style())
                .size
                .max(ui.spacing().interact_size.y);

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.autoplay, "Autoplay").on_hover_text(format!("Automatically plays all the sounds in sequence.\nSkips to the next file each {:.1} seconds", self.autoplay_interval));
                egui::DragValue::new(&mut self.autoplay_interval).speed(0.1).range(0.2f32..=5f32).max_decimals(1).ui(ui);
                ui.label("Autoplay Interval");
            });

            let available_height = ui.available_height();
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .column(Column::auto().at_least(48.0))
                .column(Column::auto().at_least(128.0))
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height);

            if row_changed {
                table = table.scroll_to_row(self.current_row, None);
            }

            ctx.style_mut(|s| {
                s.interaction.show_tooltips_only_when_still = false;
                s.interaction.tooltip_delay = 0.0;
            });
            table
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.strong("#");
                    });
                    header.col(|ui| {
                        ui.strong("Tag");
                    });
                    header.col(|ui| {
                        ui.monospace("Duration");
                    });
                })
                .body(|mut body| {
                    body.rows(text_height, audio.streams.len(), |mut row| {
                        row.set_selected(row.index() == self.current_row);
                        let (tag, duration) = &audio.streams[row.index()];
                        let mut move_row = false;
                        row.col(|ui| {
                            move_row |= ui.label(tag.entry_index().to_string()).clicked();
                        });
                        row.col(|ui| {
                            let mut s = ui.label(tag.to_string());
                            s.context_menu(|ui| tag_context(ui, *tag));
                            move_row |= s.clicked();
                        });
                        row.col(|ui| {
                            move_row |= ui.label(format_duration(*duration)).clicked();
                        });

                        if move_row {
                            self.current_row = row.index();
                            row_changed |= true;
                        }
                    })
                });

            if let Some((t, _)) = audio.streams.get(self.current_row) {
                AudioPlayer::instance().play(*t);
            }
        }

        None
    }
}

fn format_duration(d: f32) -> String {
    if d > 60.0 {
        format!(
            "{:02}:{:02}m",
            (d / 60.0).floor() as usize,
            (d % 60.0) as usize
        )
    } else {
        format!("{:02}.{:03}s", d as usize, (d * 1000.0) as usize % 1000)
    }
}
