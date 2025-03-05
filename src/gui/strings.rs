use std::{
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    sync::Arc,
};

use binrw::BinReaderExt;

use destiny_pkg::{GameVersion, TagHash};
use eframe::egui::{self, RichText};
use itertools::Itertools;
use rustc_hash::FxHashMap;

use crate::{
    package_manager::package_manager,
    scanner::TagCache,
    tagtypes::TagType,
    text::{decode_text, StringCache, StringCacheVec, StringContainer, StringData, StringPart},
};

use super::{
    audio_list::wwise_bank_type, common::ResponseExt, tag::format_tag_entry, View, ViewAction,
};

pub struct StringsView {
    cache: Arc<TagCache>,
    strings: Arc<StringCache>,
    strings_vec_filtered: StringCacheVec,

    selected_string: u32,
    string_selected_entries: Vec<(TagHash, String, TagType)>,
    string_filter: String,

    exact_match: bool,
    case_sensitive: bool,
    hide_devalpha_str: bool,
    variant: StringViewVariant,
}

#[derive(Clone, Copy, PartialEq)]
pub enum StringViewVariant {
    LocalizedStrings,
    RawWordlist,
}

impl StringsView {
    pub fn new(
        strings: Arc<StringCache>,
        cache: Arc<TagCache>,
        variant: StringViewVariant,
    ) -> Self {
        // if variant == StringViewVariant::RawWordlist {
        //     let (wwise_bank_type, wwise_bank_subtype) = wwise_bank_type();
        //     let banks = package_manager()
        //         .get_all_by_type(wwise_bank_type, Some(wwise_bank_subtype))
        //         .iter()
        //         // .filter(|(t, _e)| t.pkg_id() == id)
        //         .map(|(t, e)| (*t, e.reference))
        //         .collect_vec();

        //     for (bt, be) in banks {
        //         if let Some(name) = strings.get(&be) {
        //             println!("{bt}: '{}'", name.first().cloned().unwrap_or_default());
        //         } else {
        //             println!("{bt}: <{be:08X}>");
        //         }
        //     }
        // }

        let devstr_regex = regex::Regex::new(r"^str[0-9]*").unwrap();
        let mut strings_vec_filtered: StringCacheVec =
            strings.iter().map(|(k, v)| (*k, v.clone())).collect();

        let hide_devalpha_str = package_manager().version == GameVersion::DestinyInternalAlpha;
        if hide_devalpha_str {
            strings_vec_filtered.retain(|(_, s)| !devstr_regex.is_match(&s[0]));
        }

        Self {
            cache,
            strings,
            strings_vec_filtered,
            selected_string: u32::MAX,
            string_filter: String::new(),
            string_selected_entries: vec![],
            exact_match: false,
            case_sensitive: false,
            hide_devalpha_str,
            variant,
        }
    }
}

impl View for StringsView {
    fn view(
        &mut self,
        _ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<super::ViewAction> {
        let devstr_regex = regex::Regex::new(r"^str[0-9]*").unwrap();
        egui::SidePanel::left("strings_left_panel")
            .resizable(true)
            .min_width(384.0)
            .show_inside(ui, |ui| {
                if self.variant == StringViewVariant::LocalizedStrings
                    && ui.button("Dump all languages").clicked()
                {
                    dump_all_languages().unwrap();
                }

                ui.separator();
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let mut update_search =
                        ui.text_edit_singleline(&mut self.string_filter).changed();
                    update_search |= ui.checkbox(&mut self.exact_match, "Exact match").changed();
                    update_search |= ui
                        .checkbox(&mut self.case_sensitive, "Case sensitive")
                        .changed();

                    if package_manager().version == GameVersion::DestinyInternalAlpha {
                        update_search |= ui
                            .checkbox(&mut self.hide_devalpha_str, "Hide devalpha strXX strings")
                            .changed();
                    }

                    if update_search {
                        self.strings_vec_filtered = if !self.string_filter.is_empty() {
                            let match_b = if self.case_sensitive {
                                self.string_filter.clone()
                            } else {
                                self.string_filter.to_lowercase()
                            };

                            self.strings
                                .iter()
                                .filter(|(_, s)| {
                                    s.iter().any(|s| {
                                        let match_a = if self.case_sensitive {
                                            s.clone()
                                        } else {
                                            s.to_lowercase()
                                        };

                                        if self.hide_devalpha_str && devstr_regex.is_match(s) {
                                            false
                                        } else if self.exact_match {
                                            match_a == match_b
                                        } else {
                                            match_a.contains(&match_b)
                                        }
                                    })
                                })
                                .map(|(k, v)| (*k, v.clone()))
                                .collect()
                        } else {
                            let mut strings_vec_filtered = self
                                .strings
                                .iter()
                                .map(|(k, v)| (*k, v.clone()))
                                .collect_vec();

                            if self.hide_devalpha_str {
                                strings_vec_filtered.retain(|(_, s)| !devstr_regex.is_match(&s[0]));
                            }

                            strings_vec_filtered
                        };
                    }
                });

                let string_height = {
                    let s = ui.spacing();
                    s.interact_size.y
                };

                egui::ScrollArea::vertical()
                    .max_width(ui.available_width() * 0.70)
                    .auto_shrink([false, false])
                    .show_rows(
                        ui,
                        string_height,
                        self.strings_vec_filtered.len(),
                        |ui, range| {
                            for (hash, strings) in &self.strings_vec_filtered[range] {
                                let response = if strings.len() > 1 {
                                    ui.selectable_value(
                                        &mut self.selected_string,
                                        *hash,
                                        format!(
                                            "'{}' {:08x} ({} collisions)",
                                            truncate_string_stripped(&strings[0], 192),
                                            hash,
                                            strings.len()
                                        ),
                                    )
                                    .on_hover_text(
                                        strings.iter().map(|s| s.replace('\n', "\\n")).join("\n\n"),
                                    )
                                } else {
                                    ui.selectable_value(
                                        &mut self.selected_string,
                                        *hash,
                                        format!(
                                            "'{}' {:08x}",
                                            truncate_string_stripped(&strings[0], 192),
                                            hash
                                        ),
                                    )
                                    .on_hover_text(strings[0].clone())
                                };

                                if response.clicked() {
                                    self.string_selected_entries.clear();
                                    for (tag, _) in self.cache.hashes.iter().filter(|(_, scan)| {
                                        let hashes = match self.variant {
                                            StringViewVariant::LocalizedStrings => {
                                                &scan.string_hashes
                                            }
                                            StringViewVariant::RawWordlist => &scan.wordlist_hashes,
                                        };
                                        hashes.iter().any(|c| c.hash == *hash)
                                    }) {
                                        if let Some(e) = package_manager().get_entry(*tag) {
                                            let label = format_tag_entry(*tag, Some(&e));

                                            self.string_selected_entries.push((
                                                *tag,
                                                label,
                                                TagType::from_type_subtype(
                                                    e.file_type,
                                                    e.file_subtype,
                                                ),
                                            ));
                                        }
                                    }
                                }

                                response.context_menu(|ui| {
                                    if ui.selectable_label(false, "Copy string").clicked() {
                                        ui.output_mut(|o| o.copied_text = strings[0].clone());
                                        ui.close_menu();
                                    }
                                });
                            }
                        },
                    );
            });

        egui::CentralPanel::default()
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        if self.selected_string == u32::MAX {
                            ui.label(RichText::new("No string selected").italics());
                        } else {
                            for (tag, label, tag_type) in &self.string_selected_entries {
                                if ui
                                    .add(egui::SelectableLabel::new(
                                        false,
                                        RichText::new(label).color(tag_type.display_color()),
                                    ))
                                    .tag_context(*tag)
                                    .clicked()
                                {
                                    return Some(ViewAction::OpenTag(*tag));
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

fn truncate_string_stripped(s: &str, max_length: usize) -> String {
    let s = s.replace('\n', "\\n");

    if s.len() >= max_length {
        format!("{}...", s.chars().take(max_length).collect::<String>())
    } else {
        s.to_string()
    }
}

fn dump_all_languages() -> anyhow::Result<()> {
    let prebl = matches!(
        package_manager().version,
        GameVersion::Destiny2Beta | GameVersion::Destiny2Forsaken | GameVersion::Destiny2Shadowkeep
    );
    let bl = package_manager().version == GameVersion::Destiny2BeyondLight;

    std::fs::create_dir("strings").ok();
    let mut files: FxHashMap<String, File> = Default::default();

    for (t, _) in package_manager()
        .get_all_by_reference(u32::from_be(if prebl { 0x889a8080 } else { 0xEF998080 }))
        .into_iter()
    {
        let Ok(textset_header) = package_manager().read_tag_binrw::<StringContainer>(t) else {
            continue;
        };

        for (language_code, language_tag) in textset_header.all_languages() {
            let f = files
                .entry(language_code.to_string())
                .or_insert_with(|| File::create(format!("strings/{}.txt", language_code)).unwrap());

            let Ok(data) = package_manager().read_tag(language_tag) else {
                println!("Failed to read data for language tag {language_tag} ({language_code})",);
                continue;
            };
            let mut cur = Cursor::new(&data);
            let text_data: StringData = cur.read_le_args((prebl, bl))?;

            for (combination, hash) in text_data
                .string_combinations
                .iter()
                .zip(textset_header.string_hashes.iter())
            {
                let mut final_string = String::new();

                for ip in 0..combination.part_count {
                    cur.seek(combination.data.into())?;
                    cur.seek(SeekFrom::Current(ip * 0x20))?;
                    let part: StringPart = cur.read_le()?;
                    cur.seek(part.data.into())?;
                    let mut data = vec![0u8; part.byte_length as usize];
                    cur.read_exact(&mut data)?;
                    final_string += &decode_text(&data, part.cipher_shift);
                }

                writeln!(f, "{t}:{hash:08x} : {final_string}")?;
            }
        }
    }

    Ok(())
}
