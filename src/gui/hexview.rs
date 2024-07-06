use crate::package_manager::package_manager;
use crate::references::REFERENCE_NAMES;
use crate::swap_to_ne;
use binrw::{binread, BinReaderExt, Endian};
use destiny_pkg::{GameVersion, TagHash};
use eframe::egui;
use eframe::egui::{vec2, Color32, Rgba, RichText, ScrollArea, Sense, Ui};
use itertools::Itertools;
use std::io::{Cursor, Seek, SeekFrom};

pub struct TagHexView {
    data: Vec<u8>,
    rows: Vec<DataRow>,
    array_ranges: Vec<ArrayRange>,

    mode: DataViewMode,
    detect_floats: bool,
    split_arrays: bool,
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
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<TagHash> {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.split_arrays && !self.array_ranges.is_empty() {
                    let first_array_offset = self.array_ranges[0].start as usize;
                    self.show_row_block(ui, &self.rows[..first_array_offset / 16], 0);

                    for array in &self.array_ranges {
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            let ref_label = REFERENCE_NAMES
                                .read()
                                .get(&array.class)
                                .map(|s| format!("{s} ({:08X})", array.class))
                                .unwrap_or_else(|| format!("{:08X}", array.class));

                            ui.heading(
                                RichText::new(format!(
                                    "Array {ref_label} ({} elements)",
                                    array.length
                                ))
                                .color(Color32::WHITE)
                                .strong(),
                            );
                        });

                        self.show_row_block(
                            ui,
                            &self.rows[array.data_start as usize / 16..array.end as usize / 16],
                            array.data_start as usize,
                        );
                    }
                } else {
                    self.show_row_block(ui, &self.rows, 0);
                }
            });

        None
    }

    fn show_row_block(&self, ui: &mut Ui, rows: &[DataRow], base_offset: usize) {
        for (i, row) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.strong(format!("{:08X}:", base_offset + i * 16));
                match row {
                    DataRow::Raw(data) => {
                        let string = data
                            .chunks_exact(4)
                            .map(|b| format!("{:02X} {:02X} {:02X} {:02X}", b[0], b[1], b[2], b[3]))
                            .join("  ");
                        ui.monospace(string);
                    }
                    DataRow::Float(data) => {
                        let string = data.iter().map(|f| format!("{f:<11.2}")).join("  ");
                        ui.monospace(string);

                        if data.iter().all(|&v| v >= 0.0) {
                            let needs_normalization = data.iter().any(|&v| v > 1.0);
                            let floats = if needs_normalization {
                                let factor = data.clone().into_iter().reduce(f32::max).unwrap();
                                [
                                    data[0] / factor,
                                    data[1] / factor,
                                    data[2] / factor,
                                    data[3] / factor,
                                ]
                            } else {
                                data.clone()
                            };

                            let color =
                                Rgba::from_rgb(floats[0].abs(), floats[1].abs(), floats[2].abs());

                            let (response, painter) =
                                ui.allocate_painter(vec2(16.0, 16.0), Sense::hover());

                            painter.rect_filled(response.rect, 0.0, color);
                        }
                    }
                }
            });
        }
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

        let mut all_valid_floats = floats
            .iter()
            .all(|&v| (v.is_normal() && v.abs() < 1e7 && v.abs() > 1e-10) || v == 0.0);
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

    class: u32,
    length: u64,
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
    }

    let arrays: Vec<(u64, TagArrayHeader)> = if matches!(
        package_manager().version,
        GameVersion::DestinyInternalAlpha | GameVersion::DestinyTheTakenKing
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

        array_ranges.push(ArrayRange {
            start,
            data_start,
            end: file_end,
            class: header.tagtype,
            length: header.count,
        })
    }

    for i in 0..(array_ranges.len().max(1) - 1) {
        let next_start = array_ranges.get(i + 1).map(|r| r.start).unwrap_or(file_end);
        array_ranges[i].end = next_start;
    }

    array_ranges
}

#[binread]
struct TagArrayHeader {
    pub count: u64,
    pub tagtype: u32,
}
