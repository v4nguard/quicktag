use crate::gui::common::ResponseExt;
use crate::gui::tag::{
    format_tag_entry, ExtendedScanResult, ExtendedTagHash, ScannedHashWithEntry,
};
use crate::gui::texture::TextureCache;
use crate::gui::ViewAction;
use crate::package_manager::package_manager;
use crate::scanner;
use crate::scanner::{ScannerContext, TagCache};
use crate::tagtypes::TagType;
use eframe::egui;

pub struct ExternalFileScanView {
    pub filename: String,
    file_hashes: Vec<ScannedHashWithEntry<ExtendedTagHash>>,
}

impl ExternalFileScanView {
    pub fn new(filename: String, scancontext: &ScannerContext, data: &[u8]) -> Self {
        let scanresult = scanner::scan_file(scancontext, data, true);
        let scanresult_ext = ExtendedScanResult::from_scanresult(scanresult);

        Self {
            filename,
            file_hashes: scanresult_ext.file_hashes,
        }
    }

    pub fn view(
        &mut self,
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        texture_cache: &TextureCache,
    ) -> Option<ViewAction> {
        let mut result = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            for tag in &self.file_hashes {
                if let Some(entry) = &tag.entry {
                    let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);

                    let fancy_tag = format_tag_entry(tag.hash.hash32(), Some(entry));

                    let tag_label =
                        egui::RichText::new(format!("{fancy_tag} @ 0x{:X}", tag.offset))
                            .color(tagtype.display_color());

                    let response = ui.selectable_label(false, tag_label);
                    if response
                        .tag_context_with_texture(
                            tag.hash.hash32(),
                            texture_cache,
                            tagtype.is_texture() && tagtype.is_header(),
                        )
                        .clicked()
                    {
                        result = Some(ViewAction::OpenTag(tag.hash.hash32()));
                    }
                }
            }
        });

        result
    }
}