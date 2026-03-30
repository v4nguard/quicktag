use eframe::egui::{RichText, TextWrapMode, ahash::HashMap};
use egui_extras::Column;
use itertools::Itertools;
use quicktag_core::tagtypes::TagType;
use tiger_pkg::{package::UEntryHeader, package_manager};

use crate::{
    gui::{View, ViewAction},
    util::format_file_size,
};

#[derive(Default)]
struct UsageStat {
    count: usize,
    total_size: usize,
}

impl UsageStat {
    fn add(&mut self, entry: &UEntryHeader) {
        self.count += 1;
        self.total_size += entry.file_size as usize;
    }
}

pub struct SpaceUsageView {
    by_type: Vec<(TagType, UsageStat)>,
    by_category: Vec<(TagCategory, UsageStat)>,
}

impl SpaceUsageView {
    pub fn new() -> Self {
        let mut by_type: HashMap<TagType, UsageStat> = HashMap::default();
        let mut by_category: HashMap<TagCategory, UsageStat> = HashMap::default();

        for entry in package_manager()
            .lookup
            .tag32_entries_by_pkg
            .values()
            .flatten()
        {
            let tag_type = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
            let tag_category = TagCategory::from(tag_type);

            by_type.entry(tag_type).or_default().add(entry);
            by_category.entry(tag_category).or_default().add(entry);
        }

        let mut by_type = by_type.into_iter().collect_vec();
        let mut by_category = by_category.into_iter().collect_vec();
        by_type.sort_by(|(_, a), (_, b)| b.total_size.cmp(&a.total_size));
        by_category.sort_by(|(_, a), (_, b)| b.total_size.cmp(&a.total_size));

        Self {
            by_type,
            by_category,
        }
    }
}

impl View for SpaceUsageView {
    fn view(
        &mut self,
        ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
    ) -> Option<ViewAction> {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
        ui.heading("By Category");

        const ROW_HEIGHT: f32 = 16.0;
        egui_extras::TableBuilder::new(ui)
            .id_salt("by_category")
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder())
            .striped(true)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.heading("Category");
                });
                header.col(|ui| {
                    ui.heading("Size");
                });
                header.col(|ui| {
                    ui.heading("Count");
                });
                header.col(|ui| {
                    ui.heading("%");
                });
            })
            .body(|mut body| {
                let total_size = self
                    .by_category
                    .iter()
                    .map(|(_, s)| s.total_size)
                    .sum::<usize>();
                for (category, stat) in self.by_category.iter() {
                    body.row(ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.strong(format!("{category:?}"));
                        });
                        row.col(|ui| {
                            ui.label(format_file_size(stat.total_size));
                        });
                        row.col(|ui| {
                            ui.label(stat.count.to_string());
                        });
                        row.col(|ui| {
                            ui.label(format!(
                                "{:.1}%",
                                (stat.total_size as f64 / total_size as f64) * 100.0
                            ));
                        });
                    });
                }

                body.row(ROW_HEIGHT, |mut row| {
                    row.col(|ui| {
                        ui.strong("Total");
                    });
                    row.col(|ui| {
                        ui.label(format_file_size(total_size));
                    });
                    row.col(|ui| {
                        ui.label(
                            self.by_category
                                .iter()
                                .map(|(_, s)| s.count)
                                .sum::<usize>()
                                .to_string(),
                        );
                    });
                    row.col(|ui| {
                        ui.label("");
                    });
                });
            });

        ui.separator();
        ui.heading("By Type");

        egui_extras::TableBuilder::new(ui)
            .id_salt("by_type")
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder())
            .striped(true)
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.heading("Type");
                });
                header.col(|ui| {
                    ui.heading("Size");
                });
                header.col(|ui| {
                    ui.heading("Count");
                });
                header.col(|ui| {
                    ui.heading("");
                });
            })
            .body(|mut body| {
                for (tag_type, stat) in self.by_type.iter() {
                    body.row(ROW_HEIGHT, |mut row| {
                        row.col(|ui| {
                            ui.strong(
                                RichText::new(format!("{tag_type}"))
                                    .color(tag_type.display_color()),
                            );
                        });
                        row.col(|ui| {
                            ui.label(format_file_size(stat.total_size));
                        });
                        row.col(|ui| {
                            ui.label(stat.count.to_string());
                        });
                        row.col(|ui| {
                            ui.label("");
                        });
                    });
                }
            });

        None
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TagCategory {
    Structured,
    Texture,
    Geometry,
    Shader,
    /// Wwise
    Audio,
    /// Criware
    Video,
    /// OpenType, Umbra, Havok
    MiddlewareMisc,
    Unknown,
}

impl From<TagType> for TagCategory {
    fn from(value: TagType) -> Self {
        match value {
            TagType::TextureOld { .. }
            | TagType::Texture2D { .. }
            | TagType::TextureCube { .. }
            | TagType::Texture3D { .. }
            | TagType::TextureSampler { .. }
            | TagType::TextureLargeBuffer { .. } => Self::Texture,

            TagType::VertexBuffer { .. }
            | TagType::IndexBuffer { .. }
            | TagType::ConstantBuffer { .. } => Self::Geometry,
            TagType::PixelShader { .. }
            | TagType::VertexShader { .. }
            | TagType::GeometryShader { .. }
            | TagType::ComputeShader { .. } => Self::Shader,
            TagType::WwiseInitBank | TagType::WwiseBank | TagType::WwiseStream => Self::Audio,
            TagType::Havok | TagType::OtfFontOrUmbraTome => Self::MiddlewareMisc,
            TagType::CriwareUsm => Self::Video,
            TagType::Tag | TagType::TagGlobal => Self::Structured,
            TagType::Unknown { .. } => Self::Unknown,
        }
    }
}
