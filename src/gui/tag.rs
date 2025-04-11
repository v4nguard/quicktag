use std::cell::RefCell;
use std::env::{current_dir, temp_dir};
use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::{
    collections::HashSet,
    fmt::Display,
    io::{Cursor, Seek, SeekFrom},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    common::{
        open_audio_file_in_default_application, open_tag_in_default_application, tag_context,
        ResponseExt,
    },
    View, ViewAction,
};
use crate::classes::get_class_by_id;
use crate::gui::hexview::TagHexView;
use crate::package_manager::get_hash64;
use crate::scanner::ScannedHash;
use crate::util::ui_image_rotated;
use crate::{
    package_manager::package_manager,
    scanner::{ScanResult, TagCache},
    tagtypes::TagType,
    text::StringCache,
};
use crate::{
    scanner::read_raw_string_blob, text::RawStringHashCache, texture::Texture,
    texture::TextureCache,
};
use anyhow::Context;
use binrw::{binread, BinReaderExt, Endian};
use destiny_pkg::manager::path_cache::exe_directory;
use destiny_pkg::PackagePlatform;
use destiny_pkg::{package::UEntryHeader, GameVersion, TagHash, TagHash64};
use eframe::egui::Sense;
use eframe::egui::{collapsing_header::CollapsingState, vec2, RichText, TextureId};
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::naga::{FastHashSet, FastIndexMap};
use eframe::{
    egui::{self, CollapsingHeader},
    epaint::Color32,
    wgpu,
};
use egui_extras::syntax_highlighting::CodeTheme;
use itertools::Itertools;
use log::error;
use poll_promise::Promise;
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Copy, Clone, PartialEq)]
enum TagViewMode {
    Traversal,
    Hex,
    HexReferenced,
    Float,
    Search,
}

pub struct TagView {
    cache: Arc<TagCache>,
    tag_history: Rc<RefCell<TagHistory>>,
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
    tag_data: Vec<u8>,

    scan: ExtendedScanResult,
    tag_traversal: Option<Promise<(TraversedTag, String)>>,
    traversal_depth_limit: usize,
    traversal_show_strings: bool,
    traversal_interactive: bool,
    hide_already_traversed: bool,
    start_time: Instant,

    search_tagtype: TagType,
    search_reference: u32,
    search_min_depth: usize,
    search_depth_limit: usize,
    search_package_name_filter: String,
    search_results: Vec<(TagHash, UEntryHeader)>,

    render_state: RenderState,
    texture_cache: TextureCache,
    hexview: TagHexView,
    hexview_referenced: Option<TagHexView>,
    mode: TagViewMode,

    decompiled_shader: Result<String, String>,
}

#[macro_export]
macro_rules! swap_to_ne {
    ($v:expr, $endian:ident) => {
        if $endian != Endian::NATIVE {
            $v.swap_bytes()
        } else {
            $v
        }
    };
}

impl TagView {
    pub fn create(
        cache: Arc<TagCache>,
        tag_history: Rc<RefCell<TagHistory>>,
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

        let endian = package_manager().version.endian();
        let mut data_chunks_u32 = vec![0u32; tag_data.len() / 4];
        let mut data_chunks_u64 = vec![0u64; tag_data.len() / 8];

        unsafe {
            std::ptr::copy_nonoverlapping(
                tag_data.as_ptr(),
                data_chunks_u32.as_mut_ptr() as *mut u8,
                data_chunks_u32.len() * 4,
            );
            std::ptr::copy_nonoverlapping(
                tag_data.as_ptr(),
                data_chunks_u64.as_mut_ptr() as *mut u8,
                data_chunks_u64.len() * 8,
            );
        }

        for value in data_chunks_u32.iter_mut() {
            *value = swap_to_ne!(*value, endian);
        }

        for value in data_chunks_u64.iter_mut() {
            *value = swap_to_ne!(*value, endian);
        }

        for (i, &value) in data_chunks_u32.iter().enumerate() {
            let offset = i as u64 * 4;

            if matches!(
                value,
                0x80809fbd | // Pre-BL
                0x80809fb8 | // Post-BL
                0x80800184 |
                0x80800142
            ) {
                array_offsets.push(offset + 4);
            }

            if matches!(value, 0x80800065 | 0x808000CB) {
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

        let mut arrays: Vec<(u64, TagArray)> = if matches!(
            package_manager().version,
            GameVersion::DestinyInternalAlpha | GameVersion::DestinyTheTakenKing
        ) {
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
            .lookup
            .tag64_entries
            .iter()
            .find(|(_, e)| e.hash32 == tag)
            .map(|(&h64, _)| TagHash64(h64));

        let tag_entry = package_manager().get_entry(tag)?;
        let tag_type = TagType::from_type_subtype(tag_entry.file_type, tag_entry.file_subtype);
        let scan = ExtendedScanResult::from_scanresult(cache.hashes.get(&tag).cloned()?);

        let texture = if tag_type.is_texture() && tag_type.is_header() {
            Texture::load(&render_state, tag, true).map(|t| {
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

        let hexview_referenced = if matches!(tag_type, TagType::ConstantBuffer { .. }) {
            package_manager()
                .read_tag(tag_entry.reference)
                .ok()
                .map(TagHexView::new)
        } else {
            None
        };

        let decompiled_shader = if tag_type.is_shader() && tag_type.is_header() {
            package_manager()
                .read_tag(tag_entry.reference)
                .ok()
                .map(|d| decompile_shader(&d))
                .unwrap_or(Err("Failed to read shader data".to_string()))
        } else {
            Err("Not a shader".to_string())
        };

        Some(Self {
            hexview: TagHexView::new(tag_data.clone()),
            hexview_referenced,
            mode: TagViewMode::Traversal,

            arrays,
            string_hashes,
            raw_string_hashes,
            tag,
            tag64,
            tag_type,
            tag_entry,
            tag_data,

            texture,

            scan,
            cache,
            tag_history,
            traversal_depth_limit: 16,
            tag_traversal: None,
            traversal_show_strings: false,
            traversal_interactive: false,
            hide_already_traversed: true,

            search_tagtype: TagType::Tag,
            search_reference: u32::MAX,
            search_min_depth: 0,
            search_depth_limit: 32,
            search_package_name_filter: String::new(),
            search_results: vec![],

            string_cache,
            raw_string_hash_cache,
            raw_strings,
            start_time: Instant::now(),
            render_state,
            texture_cache,
            decompiled_shader,
        })
    }

    /// Replaces this view with another tag
    pub fn open_tag(&mut self, tag: TagHash, push_history: bool) {
        if push_history {
            self.tag_history.borrow_mut().push(tag);
        }
        if let Some(mut tv) = Self::create(
            self.cache.clone(),
            self.tag_history.clone(),
            self.string_cache.clone(),
            self.raw_string_hash_cache.clone(),
            tag,
            self.render_state.clone(),
            self.texture_cache.clone(),
        ) {
            tv.traversal_depth_limit = self.traversal_depth_limit;
            tv.traversal_show_strings = self.traversal_show_strings;
            tv.traversal_interactive = self.traversal_interactive;
            tv.mode = self.mode;
            tv.search_tagtype = self.search_tagtype;
            tv.search_reference = self.search_reference;
            tv.search_depth_limit = self.search_depth_limit;

            *self = tv;
        } else {
            error!("Could not open new tag view for {tag} (tag not found in cache)");
        }
    }

    pub fn traverse_interactive_ui(
        &self,
        ui: &mut egui::Ui,
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
            is_texture = tagtype.is_texture() && tagtype.is_header();

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
                    .tag_context_with_texture(traversed.tag, &self.texture_cache, is_texture)
                    .clicked()
                {
                    if ui.input(|i| i.modifiers.ctrl)
                        && traversed
                            .entry
                            .as_ref()
                            .map(|e| TagType::from_type_subtype(e.file_type, e.file_subtype))
                            == Some(TagType::WwiseStream)
                    {
                        open_audio_file_in_default_application(traversed.tag, "wem");
                    } else {
                        open_new_tag = Some(traversed.tag);
                    }
                }
            });
        } else {
            CollapsingState::load_with_default_open(
                ui.ctx(),
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
                        .tag_context_with_texture(traversed.tag, &self.texture_cache, is_texture)
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
                        if let Some(new_tag) = self.traverse_interactive_ui(ui, t, depth + 1) {
                            open_new_tag = Some(new_tag);
                        }
                    }
                });
            });
        }

        open_new_tag
    }

    pub fn traverse_ui(&mut self, ui: &mut egui::Ui) -> Option<TagHash> {
        let mut open_new_tag = None;
        if !self.scan.successful {
            ui.heading(RichText::new("⚠ Tag data failed to read").color(Color32::YELLOW));
        }

        if self.tag_type.is_tag() {
            ui.horizontal_wrapped(|ui| {
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
                    let string_cache = self.raw_string_hash_cache.clone();
                    let depth_limit = self.traversal_depth_limit;
                    let show_strings = self.traversal_show_strings;
                    self.tag_traversal = Some(Promise::spawn_thread("traverse tags", move || {
                        traverse_tags(
                            tag,
                            depth_limit,
                            cache,
                            string_cache,
                            show_strings,
                            TraversalDirection::Down,
                        )
                    }));
                }

                if ui
                    .add_enabled(
                        self.tag_traversal
                            .as_ref()
                            .map(|v| v.poll().is_ready())
                            .unwrap_or(true),
                        egui::Button::new("Traverse ancestors"),
                    )
                    .clicked()
                {
                    let tag = self.tag;
                    let cache = self.cache.clone();
                    let string_cache = self.raw_string_hash_cache.clone();
                    let depth_limit = self.traversal_depth_limit;
                    let show_strings = self.traversal_show_strings;
                    self.tag_traversal = Some(Promise::spawn_thread("traverse tags", move || {
                        traverse_tags(
                            tag,
                            depth_limit,
                            cache,
                            string_cache,
                            show_strings,
                            TraversalDirection::Up,
                        )
                    }));
                }

                if ui.button("Copy traversal").clicked() {
                    if let Some(traversal) = self.tag_traversal.as_ref() {
                        if let Some((_, result)) = traversal.ready() {
                            ui.output_mut(|o| o.copied_text = result.clone());
                        }
                    }
                }

                ui.add(egui::DragValue::new(&mut self.traversal_depth_limit).range(1..=256));
                ui.label("Max depth");

                ui.checkbox(
                    &mut self.traversal_show_strings,
                    "Find strings (currently only shows raw strings)",
                );
                ui.checkbox(&mut self.traversal_interactive, "Interactive");
                ui.checkbox(&mut self.hide_already_traversed, "Hide already traversed");

                if let Some(traversal) = self.tag_traversal.as_ref() {
                    if let Some((trav_interactive, _)) = traversal.ready() {
                        let ctrl = ui.input(|i| i.modifiers.ctrl);
                        if ui
                            .button(format!(
                                "Dump all tag data{}",
                                if ctrl { "+non-structure tag data" } else { "" }
                            ))
                            .on_hover_text("Dumps the tag data for all tags in the traversal tree")
                            .clicked()
                        {
                            let directory = PathBuf::from("dump")
                                .join(format!("tagdump_{}", trav_interactive.tag));
                            std::fs::create_dir_all(&directory).ok();
                            if let Err(e) = Self::dump_traversed_tag_data_recursive(
                                trav_interactive,
                                &directory,
                                ctrl,
                            ) {
                                error!("Failed to dump tag data: {e:?}");
                            }
                        }
                    }
                }
            });

            if let Some(traversal) = self.tag_traversal.as_ref() {
                if let Some((trav_interactive, trav_static)) = traversal.ready() {
                    egui::ScrollArea::both()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            if self.traversal_interactive {
                                open_new_tag = open_new_tag.or(self.traverse_interactive_ui(
                                    ui,
                                    trav_interactive,
                                    0,
                                ));
                            } else {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
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
                Ok((tex, egui_texture)) => {
                    let min_dimension = ui.available_size().min_elem();
                    let size = if tex.desc.width > tex.desc.height {
                        vec2(
                            min_dimension,
                            min_dimension * tex.desc.height as f32 / tex.desc.width as f32,
                        )
                    } else {
                        vec2(
                            min_dimension * tex.desc.width as f32 / tex.desc.height as f32,
                            min_dimension,
                        )
                    } * 0.8;
                    let (response, painter) = ui.allocate_painter(size, Sense::hover());
                    ui_image_rotated(
                        &painter,
                        *egui_texture,
                        response.rect,
                        // Rotate the image if it's a cubemap
                        if tex.desc.array_size == 6 { 90. } else { 0. },
                        tex.desc.array_size == 6,
                    );

                    ui.label(tex.desc.info());

                    if let Some(ref comment) = tex.comment {
                        ui.collapsing("Texture Header", |ui| {
                            ui.weak(comment);
                        });
                    }
                }
                Err(e) => {
                    ui.colored_label(Color32::RED, "⚠ Failed to load texture");
                    ui.colored_label(Color32::RED, strip_ansi_codes(&format!("{e:?}")));
                }
            }
        } else if self.tag_type.is_shader() && self.tag_type.is_header() {
            match &self.decompiled_shader {
                Ok(d) => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            // ui.monospace(d);
                            egui_extras::syntax_highlighting::code_view_ui(
                                ui,
                                &CodeTheme::dark(),
                                &d,
                                "cpp",
                            )
                        });
                }
                Err(e) => {
                    ui.label(format!("Decompiled shader output not available: {e}"));
                }
            }
        } else {
            ui.label(RichText::new("Traversal not available for non-8080 tags").italics());
        }

        open_new_tag
    }

    pub fn floatview_ui(&mut self, ui: &mut egui::Ui) {
        if self.tag_data.len() < 16 {
            ui.label("Tag data too short to display");
            return;
        }

        let float_count = self.tag_data.len() / 4;
        let data_f32: Vec<f32> = match bytemuck::try_cast_slice(&self.tag_data[..float_count * 4]) {
            Ok(data) => data.to_vec(),
            Err(e) => {
                ui.label("Failed to convert data to floats");
                ui.label(format!("{e:?}"));
                return;
            }
        };

        for (i, row) in data_f32.chunks(4).enumerate() {
            // Check if all values are reasonable enough to be floats. Any very low/high values (with exponents) are likely not floats.

            let mut all_valid = row
                .iter()
                .all(|&v| (v.is_normal() && v.abs() < 1e7 && v.abs() > 1e-10) || v == 0.0);
            if row.iter().all(|&v| v == 0.0) {
                all_valid = false;
            }

            if all_valid {
                ui.horizontal(|ui| {
                    ui.strong(format!("{:08X}:", i * 16));
                    for &value in row {
                        ui.label(format!("{:.4}", value));
                    }
                });
            }
        }
    }

    #[must_use]
    pub fn search_ui(&mut self, ui: &mut egui::Ui) -> Option<TagHash> {
        ui.label(RichText::new("Perform a search for a specific tag type").italics());

        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Tag type")
                .selected_text(
                    RichText::new(self.search_tagtype.to_string())
                        .color(self.search_tagtype.display_color()),
                )
                .show_ui(ui, |ui| {
                    for t in TagType::all_filterable() {
                        if ui
                            .selectable_label(
                                false,
                                RichText::new(t.to_string()).color(t.display_color()),
                            )
                            .clicked()
                        {
                            self.search_tagtype = *t;
                        }
                    }
                });

            ui.add(egui::DragValue::new(&mut self.search_min_depth).range(0..=256));
            ui.label("Min depth");

            ui.add(egui::DragValue::new(&mut self.search_depth_limit).range(1..=256));
            ui.label("Max depth");

            ui.text_edit_singleline(&mut self.search_package_name_filter);
            ui.label("Package name filter");
        });

        if ui.button("Search").clicked() {
            self.search_results = perform_tagsearch(
                &self.cache,
                self.tag,
                self.search_tagtype,
                self.search_reference,
                self.search_depth_limit,
                self.search_min_depth,
            );

            if !self.search_package_name_filter.is_empty() {
                self.search_results.retain(|(tag, _)| {
                    package_manager()
                        .package_paths
                        .get(&tag.pkg_id())
                        .map(|p| {
                            p.filename
                                .to_lowercase()
                                .contains(&self.search_package_name_filter.to_lowercase())
                        })
                        .unwrap_or(false)
                });
            }
        }

        ui.separator();

        let mut result = None;
        egui::ScrollArea::vertical().show_rows(ui, 22.0, self.search_results.len(), |ui, range| {
            for (tag, entry) in &self.search_results[range] {
                let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);

                let fancy_tag = format_tag_entry(*tag, Some(entry));

                let tag_label = egui::RichText::new(fancy_tag).color(tagtype.display_color());

                let response = ui.selectable_label(false, tag_label);
                if response
                    .tag_context_with_texture(
                        *tag,
                        &self.texture_cache,
                        tagtype.is_texture() && tagtype.is_header(),
                    )
                    .clicked()
                {
                    result = Some(*tag)
                }
            }
        });

        result
    }

    pub fn dump_traversed_tag_data_recursive(
        tag: &TraversedTag,
        directory: &Path,
        dump_non_structure: bool,
    ) -> anyhow::Result<()> {
        let tag_postfix = if let Some(entry) = &tag.entry {
            let ref_postfix = get_class_by_id(entry.reference)
                .map(|c| format!("_{}", c.name))
                .unwrap_or_default();
            let tag_type =
                TagType::from_type_subtype(entry.file_type, entry.file_subtype).to_string();
            format!(
                "_{:08X}{ref_postfix}_{}",
                entry.reference,
                tag_type.replace(" ", "").replace("/", "_")
            )
        } else {
            "".to_string()
        };

        let path = directory.join(format!("{}{}.bin", tag.tag, tag_postfix));
        match package_manager().read_tag(tag.tag) {
            Ok(o) => {
                let mut file = File::create(&path).with_context(|| {
                    format!("Failed to create tag dump file ({})", path.display())
                })?;
                file.write_all(&o)
                    .context("Failed to write tag dump file")?;
            }
            Err(e) => error!("Failed to dump data for tag {}: {e:?}", tag.tag),
        }

        if let Some(entry) = &tag.entry {
            let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
            if !tagtype.is_tag() && dump_non_structure {
                return Ok(());
            }
        }

        for subtag in &tag.subtags {
            if let Some(entry) = &subtag.entry {
                let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
                if !tagtype.is_tag() && !dump_non_structure {
                    continue;
                }
            }
            if let Err(e) =
                Self::dump_traversed_tag_data_recursive(subtag, directory, dump_non_structure)
            {
                error!("Failed to traverse tag {}: {e:?}", subtag.tag);
            }
        }

        Ok(())
    }
}

impl View for TagView {
    fn view(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<ViewAction> {
        let mut open_new_tag = None;
        let mut push_history = true;

        ctx.style_mut(|s| {
            s.interaction.show_tooltips_only_when_still = false;
            s.interaction.tooltip_delay = 0.0;
        });

        ui.horizontal(|ui| {
            let mut history = self.tag_history.borrow_mut();

            ui.style_mut().spacing.button_padding = [4.0, 4.0].into();
            ui.add_enabled_ui(history.current > 0, |ui| {
                if ui.button(RichText::new("⬅").strong()).clicked() {
                    open_new_tag = history.back();
                    push_history = false;
                }
            });

            ui.add_enabled_ui((history.current + 1) < history.tags.len(), |ui| {
                if ui.button(RichText::new("➡").strong()).clicked() {
                    open_new_tag = history.forward();
                    push_history = false;
                }
            });

            egui::ComboBox::new("tag_history", "")
                .selected_text("History")
                .show_ui(ui, |ui| {
                    let mut set_current = None;
                    for (i, (tag, tag_label, tag_color)) in history.tags.iter().enumerate().rev() {
                        if ui
                            .selectable_label(
                                i == history.current,
                                RichText::new(tag_label).color(*tag_color),
                            )
                            .clicked()
                        {
                            open_new_tag = Some(*tag);
                            push_history = false;
                            set_current = Some(i);
                        }
                    }

                    if let Some(i) = set_current {
                        history.current = i;
                    }
                });
        });

        ui.heading(format_tag_entry(self.tag, Some(&self.tag_entry)))
            .context_menu(|ui| tag_context(ui, self.tag));

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

            if ui.button("Copy all hashes referencing this tag").clicked() {
                let tag_hashes_str = self
                    .scan
                    .references
                    .iter()
                    .map(|(hash, _entry)| format!("{}", hash))
                    .collect::<Vec<String>>()
                    .join("\n");

                ui.output_mut(|o| o.copied_text = tag_hashes_str);
            }
        });

        ui.separator();
        egui::SidePanel::left("tv_left_panel")
            .resizable(true)
            .min_width(256.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
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
                                FxHashMap::<TagHash, Option<UEntryHeader>>::default();
                            for (tag, entry) in &self.scan.references {
                                references_collapsed
                                    .entry(*tag)
                                    .or_insert_with(|| entry.clone());
                            }

                            for (tag, entry) in &references_collapsed {
                                let fancy_tag = format_tag_entry(*tag, entry.as_ref());
                                let response = ui.add_enabled(
                                    *tag != self.tag,
                                    egui::SelectableLabel::new(false, fancy_tag),
                                );

                                if response.tag_context(*tag).clicked() {
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
                                if tag.hash.hash32() != self.tag {
                                    let response = ui.selectable_label(false, tag_label);
                                    if response
                                        .tag_context_with_texture(
                                            tag.hash.hash32(),
                                            &self.texture_cache,
                                            is_texture,
                                        )
                                        .clicked()
                                    {
                                        if ui.input(|i| i.modifiers.ctrl)
                                            && tag.entry.as_ref().map(|e| {
                                                TagType::from_type_subtype(
                                                    e.file_type,
                                                    e.file_subtype,
                                                )
                                            }) == Some(TagType::WwiseStream)
                                        {
                                            open_audio_file_in_default_application(
                                                tag.hash.hash32(),
                                                "wem",
                                            );
                                        } else {
                                            open_new_tag = Some(tag.hash.hash32());
                                        }
                                    }
                                }
                            }
                        }
                    });
                });
            });

        if !self.string_hashes.is_empty()
            || !self.raw_strings.is_empty()
            || !self.raw_string_hashes.is_empty()
            || !self.arrays.is_empty()
        {
            egui::SidePanel::right("tv_right_panel")
                .resizable(true)
                .min_width(320.0)
                .show_inside(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        CollapsingHeader::new(egui::RichText::new("Arrays").strong())
                            .default_open(true)
                            .show(ui, |ui| {
                                ui.group(|ui| {
                                    if self.arrays.is_empty() {
                                        ui.label(RichText::new("No arrays found").italics());
                                    } else {
                                        for (offset, array) in &self.arrays {
                                            let ref_label = get_class_by_id(array.tagtype)
                                                .map(|c| {
                                                    format!("{} ({:08X})", c.name, array.tagtype)
                                                })
                                                .unwrap_or_else(|| {
                                                    format!("{:08X}", array.tagtype)
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
                                                let (response, is_from_wordlist) =
                                                    if strings.len() > 1 {
                                                        let current_string =
                                                            (self.start_time.elapsed().as_secs()
                                                                as usize)
                                                                % strings.len();
                                                        let color = if strings[current_string].1 {
                                                            Color32::from_rgb(0, 128, 255)
                                                        } else {
                                                            Color32::GRAY
                                                        };

                                                        let response = ui
                                                            .selectable_label(
                                                                false,
                                                                RichText::new(format!(
                                                            "'{}' ({} collisions) {:08x} @ 0x{:X}",
                                                            &strings[current_string].0,
                                                            strings.len(),
                                                            hash,
                                                            offset
                                                        ))
                                                                .color(color),
                                                            )
                                                            .on_hover_text(
                                                                strings
                                                                    .iter()
                                                                    .map(|(s, _)| s)
                                                                    .join("\n"),
                                                            );

                                                        (response, strings[current_string].1)
                                                    } else {
                                                        let color = if strings[0].1 {
                                                            Color32::from_rgb(0, 128, 255)
                                                        } else {
                                                            Color32::GRAY
                                                        };
                                                        let response = ui.selectable_label(
                                                            false,
                                                            RichText::new(format!(
                                                                "'{}' {:08x} @ 0x{:X}",
                                                                strings[0].0, hash, offset
                                                            ))
                                                            .color(color),
                                                        );

                                                        (response, strings[0].1)
                                                    };

                                                if is_from_wordlist {
                                                    response.on_hover_text(
                                                        RichText::new(
                                                            "This string is from wordlist.txt",
                                                        )
                                                        .color(Color32::from_rgb(0, 128, 255)),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                });
                            });
                    });
                });
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.mode, TagViewMode::Traversal, "Traversal");
                ui.selectable_value(&mut self.mode, TagViewMode::Hex, "Hex");
                ui.selectable_value(&mut self.mode, TagViewMode::Float, "Floating point");
                if self.hexview_referenced.is_some() {
                    ui.selectable_value(
                        &mut self.mode,
                        TagViewMode::HexReferenced,
                        "Hex (referenced data)",
                    );
                }
                ui.selectable_value(&mut self.mode, TagViewMode::Search, "Search");
            });

            ui.separator();

            match self.mode {
                TagViewMode::Traversal => {
                    open_new_tag = open_new_tag.or(self.traverse_ui(ui));
                }
                TagViewMode::Hex => {
                    open_new_tag = open_new_tag.or(self.hexview.show(ui, &self.scan));
                }
                TagViewMode::HexReferenced => {
                    if let Some(h) = self.hexview_referenced.as_mut() {
                        open_new_tag = open_new_tag.or(h.show(ui, &self.scan));
                    } else {
                        self.mode = TagViewMode::Hex;
                    }
                }
                TagViewMode::Float => {
                    self.floatview_ui(ui);
                }
                TagViewMode::Search => {
                    open_new_tag = open_new_tag.or(self.search_ui(ui));
                }
            }
        });

        ctx.request_repaint_after(Duration::from_secs(1));

        if let Some(new_tag) = open_new_tag {
            self.open_tag(new_tag, push_history);
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

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum ExtendedTagHash {
    Hash32(TagHash),
    Hash64(TagHash64),
}

impl ExtendedTagHash {
    pub fn hash32(&self) -> TagHash {
        match self {
            ExtendedTagHash::Hash32(h) => *h,
            ExtendedTagHash::Hash64(h) => package_manager()
                .lookup
                .tag64_entries
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

impl Hash for ExtendedTagHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ExtendedTagHash::Hash32(h) => state.write_u32(h.0),
            ExtendedTagHash::Hash64(h) => state.write_u64(h.0),
        }
    }
}

pub struct ExtendedScanResult {
    pub successful: bool,
    pub file_hashes: Vec<ScannedHashWithEntry<ExtendedTagHash>>,

    /// References from other files
    pub references: Vec<(TagHash, Option<UEntryHeader>)>,
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
                        .lookup
                        .tag64_entries
                        .get(&s.hash.0)
                        .map(|v| v.hash32)
                        .unwrap_or(TagHash::NONE),
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
                .map(|t| (t, package_manager().get_entry(t)))
                .collect(),
        }
    }
}

pub struct ScannedHashWithEntry<T: Sized> {
    pub offset: u64,
    pub hash: T,
    pub entry: Option<UEntryHeader>,
}

#[derive(Copy, Clone, PartialEq)]
enum TraversalDirection {
    Up,
    Down,
}

/// Traverses down every tag to make a hierarchy of tags
fn traverse_tags(
    starting_tag: TagHash,
    depth_limit: usize,
    cache: Arc<TagCache>,
    raw_strings: Arc<RawStringHashCache>,
    show_strings: bool,
    direction: TraversalDirection,
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
        raw_strings,
        show_strings,
        direction,
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
    raw_strings_cache: Arc<RawStringHashCache>,
    show_strings: bool,
    direction: TraversalDirection,
) -> TraversedTag {
    let depth = pipe_stack.len();

    let pm = package_manager();

    seen_tags.insert(tag);

    let entry = pm.get_entry(tag);
    let fancy_tag = format_tag_entry(tag, entry.as_ref());
    writeln!(out, "{fancy_tag} @ 0x{offset:X}",).ok();

    if let Some(entry) = &entry {
        // ??? and localized string data
        if matches!(entry.reference, 0x808099F1 | 0x80809A8A) {
            return TraversedTag {
                tag,
                entry: Some(entry.clone()),
                reason: Some(format!(
                    "Reference 0x{:08X} is blocked from being scanned",
                    entry.reference
                )),
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

    let all_hashes = if direction == TraversalDirection::Down {
        scan.file_hashes
            .iter()
            .map(|v| (v.hash.hash32(), v.offset))
            .collect_vec()
    } else {
        let references_collapsed: FxHashSet<TagHash> =
            scan.references.iter().map(|(t, _)| *t).collect();
        references_collapsed.iter().map(|t| (*t, 0)).collect_vec()
    };

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
        let mut raw_string_hashes = vec![];
        for (i, b) in tag_data.chunks_exact(4).enumerate() {
            let v: [u8; 4] = b.try_into().unwrap();
            let hash = u32::from_le_bytes(v);

            if let Some(v) = raw_strings_cache.get(&hash) {
                raw_string_hashes.push(v[0].clone());
            }

            if hash == 0x80800065 {
                raw_strings.extend(read_raw_string_blob(&tag_data, i as u64 * 4));
            }
        }

        if !raw_strings.is_empty() {
            writeln!(
                out,
                "{line_header}├──Raw Strings: [{}]",
                raw_strings.into_iter().map(|(_, string)| string).join(", ")
            )
            .ok();
        }

        if !raw_string_hashes.is_empty() {
            writeln!(
                out,
                "{line_header}├──Raw String Hashes: [{}]",
                raw_string_hashes
                    .into_iter()
                    .map(|(string, _)|
                    //     format!("{} (wordlist.txt)", string)
                        string)
                    .join(", ")
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
            } else if *t == tag {
                // We don't care about self references
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
                raw_strings_cache.clone(),
                show_strings,
                direction,
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
            .lookup
            .named_tags
            .iter()
            .find(|v| v.hash == tag)
            .map(|v| format!("{} ", v.name))
            .unwrap_or_default();

        let ref_label = get_class_by_id(entry.reference)
            .map(|c| format!(" ({})", c.name))
            .unwrap_or_default();

        format!(
            "{}{named_tag}{tag} {}{ref_label} ({}+{}, ref {:08X})",
            if get_hash64(tag).is_some() {
                "★ "
            } else {
                ""
            },
            TagType::from_type_subtype(entry.file_type, entry.file_subtype),
            entry.file_type,
            entry.file_subtype,
            entry.reference,
        )
    } else {
        format!("{} (pkg entry not found)", tag)
    }
}

#[binread]
pub struct TagArray {
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

pub fn strip_ansi_codes(input: &str) -> String {
    let ansi_escape_pattern = regex::Regex::new(r"\x1B\[[0-9;]*[mK]").unwrap();
    ansi_escape_pattern.replace_all(input, "").to_string()
}

#[derive(Default)]
pub struct TagHistory {
    pub tags: Vec<(TagHash, String, Color32)>,
    pub current: usize,
}

impl TagHistory {
    pub fn push(&mut self, tag: TagHash) {
        self.tags.truncate(self.current + 1);

        if let Some(entry) = package_manager().get_entry(tag) {
            let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
            let color = tagtype.display_color();
            let fancy_tag = format_tag_entry(tag, Some(&entry));

            self.tags.push((tag, fancy_tag, color));
        } else {
            self.tags.push((
                tag,
                format!("{tag} (pkg entry not found)"),
                Color32::LIGHT_RED,
            ));
        }

        self.current = self.tags.len().saturating_sub(1);
    }

    pub fn back(&mut self) -> Option<TagHash> {
        if self.current > 0 {
            self.current -= 1;
            self.tags.get(self.current).map(|v| v.0)
        } else {
            None
        }
    }

    pub fn forward(&mut self) -> Option<TagHash> {
        if (self.current + 1) < self.tags.len() {
            self.current += 1;
            self.tags.get(self.current).map(|v| v.0)
        } else {
            None
        }
    }
}

fn perform_tagsearch(
    cache: &TagCache,
    start_tag: TagHash,
    tagtype: TagType,
    reference: u32,
    max_depth: usize,
    min_depth: usize,
) -> Vec<(TagHash, UEntryHeader)> {
    let results = search_for_tag(
        cache,
        start_tag,
        tagtype,
        reference,
        0,
        max_depth,
        &mut FastHashSet::default(),
    );

    // Remove any duplicates, but keep the order by using an indexmap
    let results_filtered: FastIndexMap<TagHash, UEntryHeader> = results
        .into_iter()
        .filter(|(_, _, depth)| *depth > min_depth)
        .map(|(a, b, _)| (a, b))
        .collect();

    results_filtered.into_iter().collect()
}

fn search_for_tag(
    cache: &TagCache,
    tag: TagHash,
    target_tagtype: TagType,
    target_reference: u32,
    depth: usize,
    max_depth: usize,
    seen: &mut FastHashSet<TagHash>,
) -> Vec<(TagHash, UEntryHeader, usize)> {
    if depth > max_depth {
        return vec![];
    }

    let Some(references) = cache.hashes.get(&tag) else {
        return vec![];
    };

    let mut results = vec![];

    let mut hashes = references.file_hashes.clone();
    hashes.extend(references.file_hashes64.iter().map(|r| ScannedHash {
        offset: r.offset,
        hash: ExtendedTagHash::Hash64(r.hash).hash32(),
    }));

    for r in &hashes {
        if seen.contains(&r.hash) {
            continue;
        }

        seen.insert(r.hash);

        if let Some(entry) = package_manager().get_entry(r.hash) {
            let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
            if tagtype == target_tagtype {
                results.push((r.hash, entry, depth));
            } else if tagtype.is_tag() {
                // Pesky material impact/footstep tags
                if !matches!(entry.reference, 0x8080873D | 0x8080873F) {
                    results.extend(search_for_tag(
                        cache,
                        r.hash,
                        target_tagtype,
                        target_reference,
                        depth + 1,
                        max_depth,
                        seen,
                    ));
                }
            }
        }
    }

    results
}

fn decompile_shader(data: &[u8]) -> Result<String, String> {
    if !matches!(
        package_manager().platform,
        PackagePlatform::Tool32
            | PackagePlatform::Win32
            | PackagePlatform::Win64
            | PackagePlatform::Tool64
            | PackagePlatform::Win64v1
            | PackagePlatform::XboxOne
            | PackagePlatform::Scarlett
    ) {
        return Err("Decompilation is not supported on this platform".to_string());
    }

    hlsldecompiler::decompile(data)
}
