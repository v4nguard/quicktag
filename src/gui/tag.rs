use std::{
    collections::HashSet,
    fmt::Display,
    io::{Cursor, Seek, SeekFrom},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use binrw::{binread, BinReaderExt, Endian};
use destiny_pkg::{package::UEntryHeader, PackageVersion, TagHash, TagHash64};
use eframe::egui::load::SizedTexture;
use eframe::egui::{collapsing_header::CollapsingState, vec2, TextureId};
use eframe::egui_wgpu::RenderState;
use eframe::{
    egui::{self, CollapsingHeader, RichText},
    epaint::Color32,
    wgpu,
};
use itertools::Itertools;
use log::error;
use poll_promise::Promise;
use rustc_hash::FxHashMap;
use std::fmt::Write;

use crate::{gui::texture::Texture, scanner::read_raw_string_blob, text::RawStringHashCache};
use crate::{
    packages::package_manager,
    references::REFERENCE_NAMES,
    scanner::{ScanResult, TagCache},
    tagtypes::TagType,
    text::StringCache,
};

use super::{
    common::{
        open_audio_file_in_default_application, open_tag_in_default_application, tag_context,
        ResponseExt,
    },
    texture::TextureCache,
    View, ViewAction,
};

pub struct TagView {
    cache: Arc<TagCache>,
    string_cache: Arc<StringCache>,
    raw_string_hash_cache: Arc<RawStringHashCache>,

    string_hashes: Vec<(u64, u32)>,
    raw_string_hashes: Vec<(u64, u32)>,
    raw_strings: Vec<(u64, String, Vec<u64>)>,
    arrays: Vec<(u64, TagArray)>,

    /// Used if this tag is a texture header
    texture: anyhow::Result<(Texture, TextureId)>,

    tag: TagHash,
    tag64: Option<TagHash64>,
    tag_entry: UEntryHeader,
    tag_type: TagType,

    scan: ExtendedScanResult,
    tag_traversal: Option<Promise<(TraversedTag, String)>>,
    traversal_depth_limit: usize,
    traversal_show_strings: bool,
    traversal_interactive: bool,
    hide_already_traversed: bool,
    start_time: Instant,

    render_state: RenderState,
    texture_cache: TextureCache,
}

impl TagView {
    pub fn create(
        cache: Arc<TagCache>,
        string_cache: Arc<StringCache>,
        raw_string_hash_cache: Arc<RawStringHashCache>,
        tag: TagHash,
        render_state: RenderState,
        texture_cache: TextureCache,
    ) -> Option<TagView> {
        let tag_data = package_manager().read_tag(tag).ok()?;
        let mut array_offsets = vec![];
        let mut raw_string_offsets = vec![];
        let mut string_hashes = vec![];
        let mut raw_string_hashes = vec![];

        macro_rules! swap_to_ne {
            ($v:ident, $endian:ident) => {
                if $endian != Endian::NATIVE {
                    $v.swap_bytes()
                } else {
                    $v
                }
            };
        }

        let endian = package_manager().version.endian();
        let data_chunks_u32 = bytemuck::cast_slice::<u8, u32>(&tag_data[0..tag_data.len() & !3])
            .iter()
            .map(|&v| swap_to_ne!(v, endian))
            .collect_vec();
        let data_chunks_u64 = bytemuck::cast_slice::<u8, u64>(&tag_data[0..tag_data.len() & !7])
            .iter()
            .map(|&v| swap_to_ne!(v, endian))
            .collect_vec();

        for (i, &value) in data_chunks_u32.iter().enumerate() {
            let offset = i as u64 * 4;

            if matches!(value, 0x80809fb8 | 0x80800184) {
                array_offsets.push(offset + 4);
            }

            if value == 0x80800065 {
                raw_string_offsets.push(offset);
            }

            if string_cache.contains_key(&value) {
                string_hashes.push((offset, value));
            }

            if raw_string_hash_cache.contains_key(&value) {
                raw_string_hashes.push((offset, value));
            }
        }

        let raw_strings = raw_string_offsets
            .into_iter()
            .flat_map(|o| read_raw_string_blob(&tag_data, o))
            .collect_vec();

        let raw_strings = raw_strings
            .into_iter()
            .map(|(o, s)| (o, s, find_potential_relpointers(&data_chunks_u64, o)))
            .collect_vec();

        let mut arrays: Vec<(u64, TagArray)> =
            if package_manager().version == PackageVersion::DestinyTheTakenKing {
                array_offsets
                    .into_iter()
                    .filter_map(|o| {
                        let mut c = Cursor::new(&tag_data);
                        c.seek(SeekFrom::Start(o)).ok()?;
                        Some((
                            o,
                            TagArray {
                                count: c.read_be::<u32>().ok()? as _,
                                tagtype: c.read_be::<u32>().ok()?,
                                references: vec![],
                            },
                        ))
                    })
                    .collect_vec()
            } else {
                array_offsets
                    .into_iter()
                    .filter_map(|o| {
                        let mut c = Cursor::new(&tag_data);
                        c.seek(SeekFrom::Start(o)).ok()?;
                        Some((o, c.read_le().ok()?))
                    })
                    .collect_vec()
            };

        let mut cur = Cursor::new(&tag_data);
        loop {
            let offset = cur.stream_position().unwrap();
            let Ok((value1, value2)) = cur.read_le::<(u64, u64)>() else {
                break;
            };

            let possibly_count = value1;
            let possibly_array_offset = (offset + 8).saturating_add(value2);

            if let Some((_, array)) = arrays.iter_mut().find(|(offset, arr)| {
                *offset == possibly_array_offset && arr.count == possibly_count
            }) {
                array.references.push(offset);
            }

            cur.seek(SeekFrom::Current(-8)).unwrap();
        }

        let tag64 = package_manager()
            .hash64_table
            .iter()
            .find(|(_, e)| e.hash32 == tag)
            .map(|(&h64, _)| TagHash64(h64));

        let tag_entry = package_manager().get_entry(tag)?;
        let tag_type = TagType::from_type_subtype(tag_entry.file_type, tag_entry.file_subtype);
        let scan = ExtendedScanResult::from_scanresult(cache.hashes.get(&tag).cloned()?);

        let texture = if tag_type.is_texture() && tag_type.is_header() {
            Texture::load(&render_state, tag).map(|t| {
                let egui_handle = render_state.renderer.write().register_native_texture(
                    &render_state.device,
                    &t.view,
                    wgpu::FilterMode::Linear,
                );

                (t, egui_handle)
            })
        } else {
            Err(anyhow::anyhow!("Tag is not a texture header"))
        };

        Some(Self {
            arrays,
            string_hashes,
            raw_string_hashes,
            tag,
            tag64,
            tag_type,
            tag_entry,

            texture,

            scan,
            cache,
            traversal_depth_limit: 16,
            tag_traversal: None,
            traversal_show_strings: false,
            traversal_interactive: false,
            hide_already_traversed: true,
            string_cache,
            raw_string_hash_cache,
            raw_strings,
            start_time: Instant::now(),
            render_state,
            texture_cache,
        })
    }

    /// Replaces this view with another tag
    pub fn open_tag(&mut self, tag: TagHash) {
        if let Some(mut tv) = Self::create(
            self.cache.clone(),
            self.string_cache.clone(),
            self.raw_string_hash_cache.clone(),
            tag,
            self.render_state.clone(),
            self.texture_cache.clone(),
        ) {
            tv.traversal_depth_limit = self.traversal_depth_limit;
            tv.traversal_show_strings = self.traversal_show_strings;
            tv.traversal_interactive = self.traversal_interactive;

            *self = tv;
        } else {
            error!("Could not open new tag view for {tag} (tag not found in cache)");
        }
    }

    pub fn traverse_interactive_ui(
        &self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
        traversed: &TraversedTag,
        depth: usize,
    ) -> Option<TagHash> {
        let mut open_new_tag = None;
        let mut is_texture = false;

        if self.hide_already_traversed && traversed.reason.is_some() {
            return None;
        }

        let tag_label = if let Some(entry) = &traversed.entry {
            let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
            is_texture = tagtype.is_texture();

            let fancy_tag = format_tag_entry(traversed.tag, Some(entry));
            let reason = traversed
                .reason
                .as_ref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();

            egui::RichText::new(format!("{fancy_tag}{reason}")).color(tagtype.display_color())
        } else {
            egui::RichText::new(format!("{} (pkg entry not found)", traversed.tag))
                .color(Color32::LIGHT_RED)
        };

        ui.style_mut().spacing.indent = 16.0;
        if traversed.subtags.is_empty() {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(depth > 0, egui::SelectableLabel::new(false, tag_label))
                    .tag_context_with_texture(traversed.tag, None, &self.texture_cache, is_texture)
                    .clicked()
                {
                    open_new_tag = Some(traversed.tag);
                }
            });
        } else {
            CollapsingState::load_with_default_open(
                ctx,
                egui::Id::new(format!(
                    "traversed_tag{}_collapse_depth{depth}",
                    traversed.tag
                )),
                true,
            )
            .show_header(ui, |ui| {
                ui.horizontal(|ui| {
                    let response =
                        ui.add_enabled(depth > 0, egui::SelectableLabel::new(false, tag_label));

                    if response
                        .tag_context_with_texture(
                            traversed.tag,
                            None,
                            &self.texture_cache,
                            is_texture,
                        )
                        .clicked()
                    {
                        open_new_tag = Some(traversed.tag);
                    }
                });
            })
            .body_unindented(|ui| {
                ui.style_mut().spacing.indent = 16.0 * 2.;
                ui.indent(format!("traversed_tag{}_indent", traversed.tag), |ui| {
                    for t in &traversed.subtags {
                        if let Some(new_tag) = self.traverse_interactive_ui(ctx, ui, t, depth + 1) {
                            open_new_tag = Some(new_tag);
                        }
                    }
                });
            });
        }

        open_new_tag
    }
}

impl View for TagView {
    fn view(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<ViewAction> {
        let mut open_new_tag = None;

        ui.heading(format_tag_entry(self.tag, Some(&self.tag_entry)))
            .context_menu(|ui| tag_context(ui, self.tag, self.tag64));

        ui.label(
            RichText::new(format!(
                "Package {}",
                package_manager()
                    .package_paths
                    .get(&self.tag.pkg_id())
                    .map(|p| Path::new(&p.path).file_name().unwrap_or_default())
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .weak(),
        );

        ui.horizontal(|ui| {
            if ui.button("Open tag data in external application").clicked() {
                open_tag_in_default_application(self.tag);
            }

            if self.tag_type == TagType::WwiseStream && ui.button("Play audio").clicked() {
                open_audio_file_in_default_application(self.tag, "wem");
            }

            if TagHash(self.tag_entry.reference).is_pkg_file()
                && ui
                    .button("Open referenced in external application")
                    .clicked()
            {
                open_tag_in_default_application(self.tag_entry.reference.into());
            }
        });

        ui.separator();
        egui::SidePanel::left("tv_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap = Some(false);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    CollapsingHeader::new(
                        egui::RichText::new("Files referencing this tag").strong(),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        if self.scan.references.is_empty() {
                            ui.label(RichText::new("No incoming references found").italics());
                        } else {
                            let mut references_collapsed =
                                FxHashMap::<TagHash, UEntryHeader>::default();
                            for (tag, entry) in &self.scan.references {
                                references_collapsed
                                    .entry(*tag)
                                    .or_insert_with(|| entry.clone());
                            }

                            for (tag, entry) in &references_collapsed {
                                let fancy_tag = format_tag_entry(*tag, Some(entry));
                                let response = ui.add_enabled(
                                    *tag != self.tag,
                                    egui::SelectableLabel::new(false, fancy_tag),
                                );

                                if response.tag_context(*tag, None).clicked() {
                                    open_new_tag = Some(*tag);
                                }
                            }
                        }
                    });

                    CollapsingHeader::new(
                        egui::RichText::new("Tag references in this file").strong(),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        if self.scan.file_hashes.is_empty() {
                            ui.label(RichText::new("No outgoing references found").italics());
                        } else {
                            for tag in &self.scan.file_hashes {
                                let mut is_texture = false;
                                let offset_label = if tag.offset == u64::MAX {
                                    "TagHeader reference".to_string()
                                } else {
                                    format!("0x{:X}", tag.offset)
                                };

                                let tag_label = if let Some(entry) = &tag.entry {
                                    let tagtype = TagType::from_type_subtype(
                                        entry.file_type,
                                        entry.file_subtype,
                                    );
                                    is_texture = tagtype.is_texture();

                                    let fancy_tag =
                                        format_tag_entry(tag.hash.hash32(), Some(entry));

                                    egui::RichText::new(format!("{fancy_tag} @ {offset_label}"))
                                        .color(tagtype.display_color())
                                } else {
                                    egui::RichText::new(format!(
                                        "{} (pkg entry not found) @ {offset_label}",
                                        tag.hash
                                    ))
                                    .color(Color32::LIGHT_RED)
                                };

                                // TODO(cohae): Highlight/jump to tag in hex viewer
                                let response = ui.add_enabled(
                                    tag.hash.hash32() != self.tag,
                                    egui::SelectableLabel::new(false, tag_label),
                                );

                                ctx.style_mut(|s| {
                                    s.interaction.show_tooltips_only_when_still = false;
                                    s.interaction.tooltip_delay = 0.0;
                                });
                                if response
                                    .tag_context_with_texture(
                                        tag.hash.hash32(),
                                        match tag.hash {
                                            ExtendedTagHash::Hash32(_) => None,
                                            ExtendedTagHash::Hash64(t) => Some(t),
                                        },
                                        &self.texture_cache,
                                        is_texture,
                                    )
                                    .clicked()
                                {
                                    open_new_tag = Some(tag.hash.hash32());
                                }
                            }
                        }
                    });
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if !self.scan.successful {
                ui.heading(RichText::new("⚠ Tag data failed to read").color(Color32::YELLOW));
            }

            if self.tag_type.is_tag() {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.tag_traversal
                                .as_ref()
                                .map(|v| v.poll().is_ready())
                                .unwrap_or(true),
                            egui::Button::new("Traverse children"),
                        )
                        .clicked()
                    {
                        let tag = self.tag;
                        let cache = self.cache.clone();
                        let depth_limit = self.traversal_depth_limit;
                        let show_strings = self.traversal_show_strings;
                        self.tag_traversal =
                            Some(Promise::spawn_thread("traverse tags", move || {
                                traverse_tags(tag, depth_limit, cache, show_strings)
                            }));
                    }

                    if ui.button("Copy traversal").clicked() {
                        if let Some(traversal) = self.tag_traversal.as_ref() {
                            if let Some((_, result)) = traversal.ready() {
                                ui.output_mut(|o| o.copied_text = result.clone());
                            }
                        }
                    }

                    ui.add(
                        egui::DragValue::new(&mut self.traversal_depth_limit).clamp_range(1..=256),
                    );
                    ui.label("Max depth");

                    ui.checkbox(
                        &mut self.traversal_show_strings,
                        "Find strings (currently only shows raw strings)",
                    );
                    ui.checkbox(&mut self.traversal_interactive, "Interactive");
                    ui.checkbox(&mut self.hide_already_traversed, "Hide already traversed");
                });

                if let Some(traversal) = self.tag_traversal.as_ref() {
                    if let Some((trav_interactive, trav_static)) = traversal.ready() {
                        egui::ScrollArea::both()
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                if self.traversal_interactive {
                                    open_new_tag = open_new_tag.or(self.traverse_interactive_ui(
                                        ctx,
                                        ui,
                                        trav_interactive,
                                        0,
                                    ));
                                } else {
                                    ui.style_mut().wrap = Some(false);
                                    ui.label(RichText::new(trav_static).monospace());
                                }
                            });
                    } else {
                        ui.spinner();
                        ui.label("Traversing tags");
                    }
                }
            } else if self.tag_type.is_texture() && self.tag_type.is_header() {
                match &self.texture {
                    Ok((t, egui_texture)) => {
                        let min_dimension = ui.available_size().min_elem();
                        let size = if t.width > t.height {
                            vec2(
                                min_dimension,
                                min_dimension * t.height as f32 / t.width as f32,
                            )
                        } else {
                            vec2(
                                min_dimension * t.width as f32 / t.height as f32,
                                min_dimension,
                            )
                        };
                        ui.image(SizedTexture {
                            id: *egui_texture,
                            size,
                        });

                        ui.label(format!(
                            "{}x{}x{} {:?}",
                            t.width, t.height, t.depth, t.format
                        ));
                    }
                    Err(e) => {
                        ui.colored_label(Color32::RED, "⚠ Failed to load texture");
                        ui.colored_label(Color32::RED, format!("{e:?}"));
                    }
                }
            } else {
                ui.label(RichText::new("Traversal not available for non-8080 tags").italics());
            }
        });

        if !self.string_hashes.is_empty() || !self.raw_strings.is_empty() || !self.arrays.is_empty()
        {
            egui::SidePanel::right("tv_right_panel")
                .resizable(true)
                .min_width(320.0)
                .show_inside(ui, |ui| {
                    ui.style_mut().wrap = Some(false);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        CollapsingHeader::new(egui::RichText::new("Arrays").strong())
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.group(|ui| {
                                    if self.arrays.is_empty() {
                                        ui.label(RichText::new("No arrays found").italics());
                                    } else {
                                        for (offset, array) in &self.arrays {
                                            let ref_label = REFERENCE_NAMES
                                                .read()
                                                .get(&array.tagtype)
                                                .map(|s| {
                                                    format!("{s} ({:08X})", array.tagtype.to_be())
                                                })
                                                .unwrap_or_else(|| {
                                                    format!("{:08X}", array.tagtype.to_be())
                                                });

                                            ui.selectable_label(
                                                false,
                                                format!(
                                                    "type={} count={} @ 0x{:X}",
                                                    ref_label, array.count, offset
                                                ),
                                            )
                                            .on_hover_text({
                                                if array.references.is_empty() {
                                                    "⚠ Array is not referenced!".to_string()
                                                } else {
                                                    format!(
                                                        "Referenced at {}",
                                                        array
                                                            .references
                                                            .iter()
                                                            .map(|o| format!("0x{o:X}"))
                                                            .join(", ")
                                                    )
                                                }
                                            })
                                            .clicked();
                                        }
                                    }
                                });
                            });

                        CollapsingHeader::new(egui::RichText::new("String Hashes").strong())
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.group(|ui| {
                                    if self.string_hashes.is_empty() {
                                        ui.label(RichText::new("No strings found").italics());
                                    } else {
                                        for (offset, hash) in &self.string_hashes {
                                            if let Some(strings) = self.string_cache.get(hash) {
                                                if strings.len() > 1 {
                                                    ui.selectable_label(
                                                        false,
                                                        format!(
                                                            "'{}' ({} collisions) {:08x} @ 0x{:X}",
                                                            strings[(self
                                                                .start_time
                                                                .elapsed()
                                                                .as_secs()
                                                                as usize)
                                                                % strings.len()],
                                                            strings.len(),
                                                            hash,
                                                            offset
                                                        ),
                                                    )
                                                    .on_hover_text(strings.join("\n"))
                                                    .clicked();
                                                } else {
                                                    ui.selectable_label(
                                                        false,
                                                        format!(
                                                            "'{}' {:08x} @ 0x{:X}",
                                                            strings[0], hash, offset
                                                        ),
                                                    )
                                                    .clicked();
                                                }
                                            }
                                        }
                                    }
                                });
                            });

                        CollapsingHeader::new(
                            egui::RichText::new("Raw strings (65008080)").strong(),
                        )
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.group(|ui| {
                                if self.raw_strings.is_empty() {
                                    ui.label(RichText::new("No raw strings found").italics());
                                } else {
                                    for (offset, string, offsets) in &self.raw_strings {
                                        ui.selectable_label(
                                            false,
                                            format!("'{}' @ 0x{:X}", string, offset),
                                        )
                                        .on_hover_text(if offsets.is_empty() {
                                            "⚠ Raw string is not referenced!".to_string()
                                        } else {
                                            format!(
                                                "Potentially referenced at {}",
                                                offsets
                                                    .iter()
                                                    .map(|o| format!("0x{o:X}"))
                                                    .join(", ")
                                            )
                                        })
                                        .context_menu(
                                            |ui| {
                                                if ui.selectable_label(false, "Copy text").clicked()
                                                {
                                                    ui.output_mut(|o| {
                                                        o.copied_text = string.clone()
                                                    });
                                                    ui.close_menu();
                                                }
                                            },
                                        );
                                    }
                                }
                            });
                        });

                        CollapsingHeader::new(egui::RichText::new("Raw String Hashes").strong())
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.group(|ui| {
                                    if self.raw_string_hashes.is_empty() {
                                        ui.label(
                                            RichText::new("No raw string hashes found").italics(),
                                        );
                                    } else {
                                        for (offset, hash) in &self.raw_string_hashes {
                                            if let Some(strings) =
                                                self.raw_string_hash_cache.get(hash)
                                            {
                                                if strings.len() > 1 {
                                                    ui.selectable_label(
                                                        false,
                                                        format!(
                                                            "'{}' ({} collisions) {:08x} @ 0x{:X}",
                                                            strings[(self
                                                                .start_time
                                                                .elapsed()
                                                                .as_secs()
                                                                as usize)
                                                                % strings.len()],
                                                            strings.len(),
                                                            hash,
                                                            offset
                                                        ),
                                                    )
                                                    .on_hover_text(strings.join("\n"))
                                                    .clicked();
                                                } else {
                                                    ui.selectable_label(
                                                        false,
                                                        format!(
                                                            "'{}' {:08x} @ 0x{:X}",
                                                            strings[0], hash, offset
                                                        ),
                                                    )
                                                    .clicked();
                                                }
                                            }
                                        }
                                    }
                                });
                            });
                    });
                });
        }

        ctx.request_repaint_after(Duration::from_secs(1));

        if let Some(new_tag) = open_new_tag {
            self.open_tag(new_tag);
        }

        None
    }
}

impl Drop for TagView {
    fn drop(&mut self) {
        if let Ok((_, egui_tex)) = self.texture {
            self.render_state.renderer.write().free_texture(&egui_tex);
            self.texture = Err(anyhow::anyhow!("Texture dropped"));
        }
    }
}

pub enum ExtendedTagHash {
    Hash32(TagHash),
    Hash64(TagHash64),
}

impl ExtendedTagHash {
    pub fn hash32(&self) -> TagHash {
        match self {
            ExtendedTagHash::Hash32(h) => *h,
            ExtendedTagHash::Hash64(h) => package_manager()
                .hash64_table
                .get(&h.0)
                .map(|v| v.hash32)
                .unwrap_or(TagHash::NONE),
        }
    }
}

impl Display for ExtendedTagHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtendedTagHash::Hash32(h) => h.fmt(f),
            ExtendedTagHash::Hash64(h) => h.fmt(f),
        }
    }
}

struct ExtendedScanResult {
    pub successful: bool,
    pub file_hashes: Vec<ScannedHashWithEntry<ExtendedTagHash>>,

    /// References from other files
    pub references: Vec<(TagHash, UEntryHeader)>,
}

impl ExtendedScanResult {
    pub fn from_scanresult(s: ScanResult) -> ExtendedScanResult {
        let mut file_hashes_combined = vec![];

        file_hashes_combined.extend(s.file_hashes.into_iter().map(|s| ScannedHashWithEntry {
            offset: s.offset,
            hash: ExtendedTagHash::Hash32(s.hash),
            entry: package_manager().get_entry(s.hash),
        }));

        file_hashes_combined.extend(s.file_hashes64.into_iter().map(|s| {
            ScannedHashWithEntry {
                offset: s.offset,
                hash: ExtendedTagHash::Hash64(s.hash),
                entry: package_manager().get_entry(
                    package_manager()
                        .hash64_table
                        .get(&s.hash.0)
                        .unwrap()
                        .hash32,
                ),
            }
        }));

        file_hashes_combined.sort_unstable_by_key(|v| v.offset);

        ExtendedScanResult {
            successful: s.successful,
            file_hashes: file_hashes_combined,
            references: s
                .references
                .into_iter()
                // TODO(cohae): Unwrap *should* be safe as long as the cache is valid but i want to be sure
                .map(|t| (t, package_manager().get_entry(t).unwrap()))
                .collect(),
        }
    }
}

struct ScannedHashWithEntry<T: Sized> {
    pub offset: u64,
    pub hash: T,
    pub entry: Option<UEntryHeader>,
}

/// Traverses down every tag to make a hierarchy of tags
fn traverse_tags(
    starting_tag: TagHash,
    depth_limit: usize,
    cache: Arc<TagCache>,
    show_strings: bool,
) -> (TraversedTag, String) {
    let mut result = String::new();
    let mut seen_tags = Default::default();
    let mut pipe_stack = vec![];

    let traversed = traverse_tag(
        &mut result,
        starting_tag,
        TagHash::NONE,
        0,
        &mut seen_tags,
        &mut pipe_stack,
        depth_limit,
        cache,
        show_strings,
    );

    (traversed, result)
}

pub struct TraversedTag {
    pub tag: TagHash,
    pub entry: Option<UEntryHeader>,
    pub reason: Option<String>,
    pub subtags: Vec<TraversedTag>,
}

#[allow(clippy::too_many_arguments)]
fn traverse_tag(
    out: &mut String,
    tag: TagHash,
    parent_tag: TagHash,
    offset: u64,
    seen_tags: &mut HashSet<TagHash>,
    pipe_stack: &mut Vec<char>,
    depth_limit: usize,
    cache: Arc<TagCache>,
    show_strings: bool,
) -> TraversedTag {
    let depth = pipe_stack.len();

    let pm = package_manager();

    seen_tags.insert(tag);

    let entry = pm.get_entry(tag);
    let fancy_tag = format_tag_entry(tag, entry.as_ref());
    writeln!(out, "{fancy_tag} @ 0x{offset:X}",).ok();

    if let Some(entry) = &entry {
        if matches!(entry.reference, 0x808099F1 | 0x80808BE0) {
            return TraversedTag {
                tag,
                entry: Some(entry.clone()),
                reason: None,
                subtags: vec![],
            };
        }
    }

    if depth >= depth_limit {
        let mut line_header = String::new();
        for s in pipe_stack.iter() {
            write!(line_header, "{s}   ").ok();
        }

        writeln!(out, "{line_header}└ Depth limit reached ({})", depth_limit).ok();

        return TraversedTag {
            tag,
            entry,
            reason: Some(format!("Depth limit reached ({depth_limit})")),
            subtags: vec![],
        };
    }

    let Some(scan_result) = cache.hashes.get(&tag).cloned() else {
        return TraversedTag {
            tag,
            entry,
            reason: Some("Tag not found in cache".to_string()),
            subtags: vec![],
        };
    };

    let scan = ExtendedScanResult::from_scanresult(scan_result);

    // writeln!(
    //     out,
    //     "{} {} ({}+{}, ref {}) @ 0x{offset:X}",
    //     tag,
    //     TagType::from_type_subtype(entry.file_type, entry.file_subtype),
    //     entry.file_type,
    //     entry.file_subtype,
    //     TagHash(entry.reference),
    // )
    // .ok();

    let all_hashes = scan
        .file_hashes
        .iter()
        .map(|v| (v.hash.hash32(), v.offset))
        .collect_vec();

    if all_hashes.is_empty() {
        return TraversedTag {
            tag,
            entry,
            reason: None,
            subtags: vec![],
        };
    }

    let mut line_header = String::new();
    for s in pipe_stack.iter() {
        write!(line_header, "{s}   ").ok();
    }

    if show_strings {
        let tag_data = package_manager().read_tag(tag).unwrap();
        let mut raw_strings = vec![];
        for (i, b) in tag_data.chunks_exact(4).enumerate() {
            let v: [u8; 4] = b.try_into().unwrap();
            let hash = u32::from_le_bytes(v);

            if hash == 0x80800065 {
                raw_strings.extend(read_raw_string_blob(&tag_data, i as u64 * 4));
            }
        }

        if !raw_strings.is_empty() {
            writeln!(
                out,
                "{line_header}├──Strings: [{}]",
                raw_strings.into_iter().map(|(_, string)| string).join(", ")
            )
            .ok();
        }
    }

    let mut subtags = vec![];
    for (i, (t, offset)) in all_hashes.iter().enumerate() {
        let branch = if i + 1 == all_hashes.len() {
            "└"
        } else {
            "├"
        };

        // Last tag, add a space instead of a pipe
        if i + 1 == all_hashes.len() {
            pipe_stack.push(' ');
        } else {
            pipe_stack.push('│');
        }

        if seen_tags.contains(t) {
            let entry = pm.get_entry(*t);
            let fancy_tag = format_tag_entry(*t, entry.as_ref());

            let offset_label = if *offset == u64::MAX {
                "TagHeader reference".to_string()
            } else {
                format!("0x{:X}", offset)
            };

            if entry
                .as_ref()
                .map(|e| e.file_type != 8 && e.file_subtype != 16)
                .unwrap_or_default()
            {
                writeln!(out, "{line_header}{branch}──{fancy_tag} @ {offset_label}").ok();

                subtags.push(TraversedTag {
                    tag: *t,
                    entry,

                    reason: None,
                    subtags: vec![],
                });
            } else if *t == parent_tag {
                writeln!(
                    out,
                    "{line_header}{branch}──{fancy_tag} @ {offset_label} (parent)"
                )
                .ok();

                // subtags.push(TraversedTag {
                //     tag: *t,
                //     entry: Right(format!("Parent")),
                //     subtags: vec![],
                // });
            } else if *t == tag {
                writeln!(
                    out,
                    "{line_header}{branch}──{fancy_tag} @ {offset_label} (self reference)"
                )
                .ok();

                // subtags.push(TraversedTag {
                //     tag: *t,
                //     entry: Right(format!("Self reference")),
                //     subtags: vec![],
                // });
            } else {
                writeln!(
                    out,
                    "{line_header}{branch}──{fancy_tag} @ {offset_label} (already traversed)"
                )
                .ok();

                subtags.push(TraversedTag {
                    tag: *t,
                    entry,

                    reason: Some("Already traversed".to_string()),
                    subtags: vec![],
                });
            }
        } else {
            write!(out, "{line_header}{branch}──").ok();
            let traversed = traverse_tag(
                out,
                *t,
                tag,
                *offset,
                seen_tags,
                pipe_stack,
                depth_limit,
                cache.clone(),
                show_strings,
            );

            subtags.push(traversed);
        }
        pipe_stack.pop();
    }

    writeln!(out, "{line_header}").ok();

    TraversedTag {
        tag,
        entry,
        reason: None,
        subtags,
    }
}

pub fn format_tag_entry(tag: TagHash, entry: Option<&UEntryHeader>) -> String {
    if let Some(entry) = entry {
        let named_tag = package_manager()
            .named_tags
            .iter()
            .find(|v| v.hash == tag)
            .map(|v| format!("{} ", v.name))
            .unwrap_or_default();

        let ref_label = REFERENCE_NAMES
            .read()
            .get(&entry.reference)
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();

        format!(
            "{named_tag}{tag} {}{ref_label} ({}+{}, ref {})",
            TagType::from_type_subtype(entry.file_type, entry.file_subtype),
            entry.file_type,
            entry.file_subtype,
            TagHash(entry.reference),
        )
    } else {
        format!("{} (pkg entry not found)", tag)
    }
}

#[binread]
struct TagArray {
    pub count: u64,
    pub tagtype: u32,

    #[br(ignore)]
    pub references: Vec<u64>,
}

fn find_potential_relpointers(data: &[u64], target_offset: u64) -> Vec<u64> {
    let mut result = vec![];

    for (i, &v) in data.iter().enumerate() {
        let offset = i as isize * 8;
        if (offset + (v as isize)) as u64 == target_offset {
            result.push(offset as u64);
        }
    }

    result
}
