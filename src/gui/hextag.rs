use destiny_pkg::TagHash;
use eframe::egui::Ui;

#[derive(Copy, Clone)]
enum DataViewMode {
    Float,
    Raw,
    U32,
}

pub struct TagHexView {
    data: Vec<u8>,
    mode: DataViewMode,
}

impl TagHexView {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            mode: DataViewMode::Raw,
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<TagHash> {
        None
    }
}
