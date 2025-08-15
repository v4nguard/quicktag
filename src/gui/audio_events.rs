use std::{io::Cursor, u32};

use binrw::{BinReaderExt, VecArgs};
use eframe::egui::{self, Color32, RichText};
use egui_extras::{Column, TableBuilder};
use quicktag_core::tagtypes::TagType;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tiger_pkg::{GameVersion, TagHash, package_manager};

use crate::gui::{
    View, ViewAction, audio_list::wwise_event_type, common::ResponseExt, get_string_for_hash,
    tag::format_tag_entry,
};

struct AudioEvent {
    tag: TagHash,
    bank_hash: u32,
    name: Option<String>,
    streams: Vec<TagHash>,
}

pub struct AudioEventView {
    events: Vec<AudioEvent>,
    selected_event: Option<(TagHash, usize)>,
}

impl AudioEventView {
    pub fn new() -> Self {
        let event_type = wwise_event_type().unwrap_or(u32::MAX);
        let tags = package_manager().get_all_by_reference(event_type);

        let mut events: Vec<AudioEvent> = tags
            .par_iter()
            .map(|(t, _)| {
                let data = package_manager().read_tag(*t).expect("Failed to read tag");
                let mut data = Cursor::new(data);
                if package_manager().version
                    >= GameVersion::Destiny(tiger_pkg::DestinyVersion::Destiny2BeyondLight)
                {
                    data.set_position(0x18);
                    let bank_tag: TagHash = data.read_le().unwrap();
                    let bank_hash = package_manager().get_entry(bank_tag).unwrap().reference;
                    let name = get_string_for_hash(bank_hash);

                    data.set_position(0x20);
                    let event_count: u64 = data.read_le().unwrap();
                    data.set_position(0x50);
                    let streams: Vec<TagHash> = data
                        .read_le_args(VecArgs::builder().count(event_count as usize).finalize())
                        .unwrap();

                    AudioEvent {
                        tag: *t,
                        bank_hash,
                        name,
                        streams,
                    }
                } else {
                    data.set_position(0x14);
                    let bank_tag: TagHash = data.read_le().unwrap();
                    let bank_hash = package_manager().get_entry(bank_tag).unwrap().reference;
                    let name = get_string_for_hash(bank_hash);
                    data.set_position(0x18);
                    let event_count: u64 = data.read_le().unwrap();
                    data.set_position(0x50);
                    let streams: Vec<TagHash> = data
                        .read_le_args(VecArgs::builder().count(event_count as usize).finalize())
                        .unwrap();

                    AudioEvent {
                        tag: *t,
                        bank_hash,
                        name,
                        streams,
                    }
                }
            })
            .collect();

        events.sort_by_cached_key(|e| {
            e.name
                .clone()
                .unwrap_or_else(|| format!("zzzz{:08X}", e.bank_hash))
        });

        AudioEventView {
            events,
            selected_event: None,
        }
    }
}

impl View for AudioEventView {
    fn view(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<super::ViewAction> {
        if let Some(action) = egui::SidePanel::right("event_stream_list")
            .min_width(320.0)
            .show_inside(ui, |ui| {
                ctx.style_mut(|s| {
                    s.interaction.show_tooltips_only_when_still = false;
                    s.interaction.tooltip_delay = 0.0;
                });
                if let Some((_, selected_event_index)) = self.selected_event {
                    let event = &self.events[selected_event_index];
                    for tag in &event.streams {
                        let entry = package_manager().get_entry(*tag);
                        let fancy_tag = format_tag_entry(*tag, entry.as_ref());

                        let tag_label =
                            egui::RichText::new(fancy_tag).color(Color32::from_rgb(191, 106, 247));

                        if ui
                            .add(egui::SelectableLabel::new(false, tag_label))
                            .tag_context_with_preview(*tag, None, false)
                            .clicked()
                        {
                            return Some(ViewAction::OpenTag(*tag));
                        }
                    }
                }

                None
            })
            .inner
        {
            return Some(action);
        }

        let text_height = egui::TextStyle::Body
            .resolve(ui.style())
            .size
            .max(ui.spacing().interact_size.y);
        let available_height = ui.available_height();
        let table = TableBuilder::new(ui)
            .striped(true)
            .column(Column::auto().at_least(96.0))
            .column(Column::auto().at_least(96.0))
            .column(Column::auto().at_least(48.0))
            .column(Column::remainder())
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height);

        // if row_changed {
        //     table = table.scroll_to_row(self.current_row, None);
        // }

        ctx.style_mut(|s| {
            s.interaction.show_tooltips_only_when_still = false;
            s.interaction.tooltip_delay = 0.0;
        });
        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Tag");
                });
                header.col(|ui| {
                    ui.strong("Hash");
                });
                header.col(|ui| {
                    ui.monospace("Streams");
                });
                header.col(|ui| {
                    ui.monospace("Name");
                });
            })
            .body(|body| {
                body.rows(text_height, self.events.len(), |mut row| {
                    let event = &self.events[row.index()];
                    if let Some((selected_event_tag, _)) = self.selected_event
                        && selected_event_tag == event.tag
                    {
                        row.set_selected(true);
                    };

                    let mut clicked = false;
                    clicked |= row
                        .col(|ui| {
                            clicked |= ui.label(event.tag.to_string()).clicked();
                        })
                        .1
                        .interact(egui::Sense::click())
                        .clicked();

                    clicked |= row
                        .col(|ui| {
                            clicked |= ui.label(format!("{:08X}", event.bank_hash)).clicked();
                        })
                        .1
                        .interact(egui::Sense::click())
                        .clicked();
                    clicked |= row
                        .col(|ui| {
                            clicked |= ui.label(event.streams.len().to_string()).clicked();
                        })
                        .1
                        .interact(egui::Sense::click())
                        .clicked();
                    clicked |= row
                        .col(|ui| {
                            clicked |= if let Some(name) = &event.name {
                                ui.label(
                                    RichText::new(name).color(Color32::from_rgb(100, 177, 255)),
                                )
                            } else {
                                ui.label(
                                    RichText::new(format!("0x{:08X}", event.bank_hash))
                                        .color(Color32::LIGHT_GRAY),
                                )
                            }
                            .clicked();
                        })
                        .1
                        .interact(egui::Sense::click())
                        .clicked();

                    if clicked {
                        self.selected_event = Some((event.tag, row.index()));
                    }
                })
            });

        None
    }
}
