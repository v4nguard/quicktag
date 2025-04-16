use crate::classes::{self, get_class_by_id};
use crate::gui::common::ResponseExt;
use crate::gui::tag::{format_tag_entry, ExtendedScanResult};
use crate::swap_to_ne;
use crate::tagtypes::TagType;
use binrw::{binread, BinReaderExt, Endian};
use eframe::egui;
use eframe::egui::{
    collapsing_header::CollapsingState, vec2, Color32, CursorIcon, Rgba, RichText, ScrollArea,
    Sense, Stroke, Ui,
};
use itertools::Itertools;
use log::warn;
use std::io::{Cursor, Seek, SeekFrom};
use tiger_pkg::package_manager;
use tiger_pkg::{DestinyVersion, GameVersion, TagHash, Version};

pub struct TagHexView {
    data: Vec<u8>,
    rows: Vec<DataRow>,
    array_ranges: Vec<ArrayRange>,

    mode: DataViewMode,
    detect_floats: bool,
    split_arrays: bool,
    raw_array_data: bool,
}

impl TagHexView {
    pub fn new(mut data: Vec<u8>) -> Self {
        // Pad data to an alignment of 16 bytes
        let remainder = data.len() % 16;
        if remainder != 0 {
            data.extend(vec![0; 16 - remainder]);
        }

        Self {
            rows: data
                .chunks_exact(16)
                .map(|chunk| DataRow::from(<[u8; 16]>::try_from(chunk).unwrap()))
                .collect(),
            array_ranges: find_all_array_ranges(&data),
            data,
            mode: DataViewMode::Auto,
            detect_floats: true,
            split_arrays: true,
            raw_array_data: false,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, scan: &ExtendedScanResult) -> Option<TagHash> {
        if self.data.len() > 1024 * 1024 * 16 {
            ui.label("Data too large to display");
            return None;
        }

        if classes::was_schemafile_refreshed() {
            self.array_ranges = find_all_array_ranges(&self.data);
        }

        ui.checkbox(&mut self.raw_array_data, "Show raw array data");
        ui.separator();

        let mut open_tag = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.split_arrays && !self.array_ranges.is_empty() {
                    let first_array_offset = self.array_ranges[0].start as usize;
                    open_tag = open_tag.or(self.show_row_block(
                        ui,
                        &self.rows[..first_array_offset / 16],
                        0,
                        scan,
                    ));

                    for (i, array) in self.array_ranges.iter().enumerate() {
                        ui.add_space(16.0);

                        CollapsingState::load_with_default_open(
                            ui.ctx(),
                            egui::Id::new(format!("hexview_array_{i}",)),
                            array.length < 256,
                        )
                        .show_header(ui, |ui| {
                            ui.horizontal(|ui| {
                                let heading = if let Some(label) = &array.label {
                                    label.clone()
                                } else {
                                    let ref_label = get_class_by_id(array.class)
                                        .map(|c| format!("{} ({:08X})", c.name, array.class))
                                        .unwrap_or_else(|| format!("{:08X}", array.class));
                                    format!("Array {ref_label} ({} elements)", array.length)
                                };

                                ui.heading(RichText::new(heading).color(Color32::WHITE).strong());
                            });
                        })
                        .body_unindented(|ui| {
                            if !self.raw_array_data && !array.pretty_rows.is_empty() {
                                let class_size = get_class_by_id(array.class).and_then(|c| c.size);
                                for (i, row) in array.pretty_rows.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        if let Some(class_size) = class_size {
                                            let offset = array.data_start as usize + i * class_size;
                                            ui.strong(format!("{:08X}:", offset));
                                        }
                                        ui.strong(format!("[{i}]"));
                                        ui.style_mut().spacing.item_spacing.x = 14.0;
                                        ui.monospace(row);
                                    });
                                }
                            } else {
                                open_tag = open_tag.or(self.show_row_block(
                                    ui,
                                    &self.rows
                                        [array.data_start as usize / 16..array.end as usize / 16],
                                    array.data_start as usize,
                                    scan,
                                ));
                            }
                        });
                    }
                } else {
                    open_tag = open_tag.or(self.show_row_block(ui, &self.rows, 0, scan));
                }
            });

        open_tag
    }

    #[must_use]
    fn show_row_block(
        &self,
        ui: &mut Ui,
        rows: &[DataRow],
        base_offset: usize,
        scan: &ExtendedScanResult,
    ) -> Option<TagHash> {
        let mut open_tag = None;
        for (i, row) in rows.iter().enumerate() {
            let offset = base_offset + i * 16;
            ui.horizontal(|ui| {
                ui.strong(format!("{:08X}:", base_offset + i * 16));
                ui.style_mut().spacing.item_spacing.x = 14.0;
                match row {
                    DataRow::Raw(data) => {
                        for (bi, b) in data.chunks_exact(4).enumerate() {
                            let chunk_offset = offset + bi * 4;
                            let hash = scan
                                .file_hashes
                                .iter()
                                .find(|v| v.offset == chunk_offset as u64);
                            let color = if hash.is_some() {
                                Color32::GOLD
                            } else {
                                Color32::GRAY
                            };

                            let response = ui.monospace(
                                RichText::new(format!(
                                    "{:02X} {:02X} {:02X} {:02X}",
                                    b[0], b[1], b[2], b[3]
                                ))
                                .color(color),
                            );
                            if let Some(e) = hash {
                                let hash32 = e.hash.hash32();
                                let tagline_color = e
                                    .entry
                                    .as_ref()
                                    .map(|e| {
                                        TagType::from_type_subtype(e.file_type, e.file_subtype)
                                            .display_color()
                                    })
                                    .unwrap_or(Color32::GRAY);
                                let response = response
                                    .on_hover_text(
                                        RichText::new(format_tag_entry(hash32, e.entry.as_ref()))
                                            .color(tagline_color),
                                    )
                                    .tag_context(hash32)
                                    .interact(Sense::click())
                                    .on_hover_cursor(CursorIcon::PointingHand);

                                if response.hovered() {
                                    ui.painter().rect(
                                        response.rect,
                                        0.0,
                                        Color32::from_white_alpha(30),
                                        Stroke::NONE,
                                    );
                                }

                                if response.clicked() {
                                    open_tag = Some(hash32);
                                }
                            }
                        }
                    }
                    DataRow::Float(data) => {
                        let string = data.iter().map(|f| fmt_short_float(*f)).join("  ");
                        ui.monospace(string);
                        ui.add_space(16.0);

                        if data.iter().all(|&v| v >= 0.0) {
                            let needs_normalization = data.iter().any(|&v| v > 1.0);
                            let floats = if needs_normalization {
                                let factor = (*data).into_iter().reduce(f32::max).unwrap();
                                [
                                    data[0] / factor,
                                    data[1] / factor,
                                    data[2] / factor,
                                    data[3] / factor,
                                ]
                            } else {
                                *data
                            };

                            let color =
                                Rgba::from_rgb(floats[0].abs(), floats[1].abs(), floats[2].abs());

                            let (response, painter) =
                                ui.allocate_painter(vec2(16.0, 16.0), Sense::hover());

                            painter.rect_filled(response.rect, 0.0, color);
                        }
                    }
                }

                if let Some(bytes) = row.as_raw() {
                    ui.add_space(16.0);
                    let (_response, painter) =
                        ui.allocate_painter(vec2(16.0 * 16.0, 16.0), Sense::hover());

                    ui.style_mut().spacing.item_spacing.x = 4.0;
                    for (i, &b) in bytes.iter().enumerate() {
                        let (c, color) = if b.is_ascii_graphic() {
                            (b as char, Color32::from_rgb(90, 120, 255))
                        } else {
                            ('.', Color32::DARK_GRAY)
                        };

                        let pos = painter.clip_rect().min + vec2(i as f32 * 12.0, 0.0);
                        painter.text(
                            pos,
                            egui::Align2::LEFT_TOP,
                            c.to_string(),
                            egui::FontId::monospace(12.0),
                            color,
                        );
                    }
                }
            });
        }

        open_tag
    }
}

#[derive(Copy, Clone)]
enum DataViewMode {
    Auto,
    Raw,
    Float,
    U32,
}

#[derive(Clone, Copy)]
enum DataRow {
    Raw([u8; 16]),
    Float([f32; 4]),
    // U32([u32; 4]),
}

impl DataRow {
    fn as_raw(&self) -> Option<&[u8; 16]> {
        match self {
            DataRow::Raw(data) => Some(data),
            _ => None,
        }
    }
}

impl From<[u8; 16]> for DataRow {
    fn from(data: [u8; 16]) -> Self {
        let from_xe_bytes = if package_manager().version.endian() == Endian::Big {
            f32::from_be_bytes
        } else {
            f32::from_le_bytes
        };

        let floats = [
            from_xe_bytes(data[0..4].try_into().unwrap()),
            from_xe_bytes(data[4..8].try_into().unwrap()),
            from_xe_bytes(data[8..12].try_into().unwrap()),
            from_xe_bytes(data[12..16].try_into().unwrap()),
        ];

        let mut all_valid_floats = floats.iter().all(|&v| {
            (v.is_normal() && v.abs() < 1e7 && v.abs() > 1e-10)
                || v == 0.0
                || v == f32::MAX
                || v == f32::MIN
        });
        if floats.iter().all(|&v| v == 0.0) {
            all_valid_floats = false;
        }

        if all_valid_floats {
            DataRow::Float(floats)
        } else {
            DataRow::Raw(data)
        }
    }
}

#[derive(Debug)]
struct ArrayRange {
    /// Start of array header
    start: u64,
    /// Start of array data
    data_start: u64,
    end: u64,

    label: Option<String>,
    class: u32,
    length: u64,

    pretty_rows: Vec<String>,
}

fn find_all_array_ranges(data: &[u8]) -> Vec<ArrayRange> {
    let mut cur = Cursor::new(data);
    let endian = package_manager().version.endian();

    let mut data_chunks_u32 = vec![0u32; data.len() / 4];

    unsafe {
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            data_chunks_u32.as_mut_ptr() as *mut u8,
            data_chunks_u32.len() * 4,
        );
    }

    for value in data_chunks_u32.iter_mut() {
        *value = swap_to_ne!(*value, endian);
    }

    let mut array_offsets = vec![];
    let mut strings_offset: Option<u64> = None;
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
            strings_offset = Some(offset + 4);
        }
    }

    let arrays: Vec<(u64, TagArrayHeader)> = if matches!(
        package_manager().version,
        GameVersion::Destiny(DestinyVersion::DestinyInternalAlpha)
            | GameVersion::Destiny(DestinyVersion::DestinyTheTakenKing)
    ) {
        array_offsets
            .into_iter()
            .filter_map(|o| {
                cur.seek(SeekFrom::Start(o)).ok()?;
                Some((
                    o,
                    TagArrayHeader {
                        count: cur.read_be::<u32>().ok()? as _,
                        tagtype: cur.read_be::<u32>().ok()?,
                    },
                ))
            })
            .collect_vec()
    } else {
        array_offsets
            .into_iter()
            .filter_map(|o| {
                cur.seek(SeekFrom::Start(o)).ok()?;
                Some((o, cur.read_le().ok()?))
            })
            .collect_vec()
    };

    let mut array_ranges = vec![];

    let file_end = data.len() as u64;
    for (offset, header) in arrays {
        let start = offset;
        let data_start = offset + 16;
        let mut pretty_rows = vec![];
        if let Some(class) = get_class_by_id(header.tagtype) {
            if class.has_pretty_formatter() {
                let class_size = class
                    .size
                    .expect("Class size zero but has a pretty formatter??");
                if data_start + (class.array_size(header.count as usize).unwrap_or_default() as u64)
                    <= file_end
                {
                    for i in 0..header.count as usize {
                        let offset = data_start as usize + i * class_size;
                        let data = &data[offset..offset + class_size];
                        pretty_rows.push(
                            class
                                .parse_and_format(data, endian)
                                .unwrap_or_else(|| "Failed to parse row".to_string()),
                        );
                    }
                } else {
                    warn!(
                        "Array data for class {:08X}/{} goes beyond file end (data_start=0x{data_start:X} array_size=0x{:X} file_end=0x{file_end:X}",
                        class.id, class.name, class.array_size(header.count as usize).unwrap_or_default()
                    );
                }
            }
        }

        array_ranges.push(ArrayRange {
            start,
            data_start,
            end: file_end,
            label: None,
            class: header.tagtype,
            length: header.count,
            pretty_rows,
        })
    }

    for i in 0..(array_ranges.len().max(1) - 1) {
        let next_start = array_ranges.get(i + 1).map(|r| r.start).unwrap_or(file_end);
        array_ranges[i].end = next_start;
    }

    if let Some(strings_offset) = strings_offset {
        let strings_offset_aligned = strings_offset & !0xf;
        if !array_ranges.is_empty() {
            array_ranges.last_mut().unwrap().end = strings_offset_aligned;
        }
        array_ranges.push(ArrayRange {
            start: strings_offset + 4,
            data_start: strings_offset + 4,
            end: file_end,
            label: Some("Raw String Data".to_string()),
            class: 0,
            length: 0,
            // TODO: would be cool maybe?
            pretty_rows: vec![],
        });
    }

    array_ranges
}

#[binread]
struct TagArrayHeader {
    pub count: u64,
    pub tagtype: u32,
}

fn fmt_short_float(f: f32) -> String {
    match f {
        f32::MAX => "f32::MAX".to_string(),
        f32::MIN => "f32::MIN".to_string(),
        _ => format!("{f:<11.6}"),
    }
}
