use crate::gui::common::ResponseExt;
use crate::gui::tag::{
    format_tag_entry, ExtendedScanResult, ExtendedTagHash, ScannedHashWithEntry,
};
use crate::gui::ViewAction;
use crate::texture::TextureCache;
use eframe::egui;
use quicktag_core::tagtypes::TagType;
use quicktag_scanner::{context::ScannerContext, ScannerMode};

pub struct ExternalFileScanView {
    pub filename: String,
    file_hashes: Vec<ScannedHashWithEntry<ExtendedTagHash>>,
}

impl ExternalFileScanView {
    pub fn new(filename: String, scancontext: &ScannerContext, data: &[u8]) -> Self {
        let scanresult = quicktag_scanner::scan_file(scancontext, data, ScannerMode::Tags);
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

        if ui.button("Copy tag list").clicked() {
            let mut taglist = String::new();

            for tag in &self.file_hashes {
                if let Some(entry) = &tag.entry {
                    // let tagtype = TagType::from_type_subtype(entry.file_type, entry.file_subtype);
                    let fancy_tag = format_tag_entry(tag.hash.hash32(), Some(entry));
                    taglist += &format!("{fancy_tag} @ 0x{:X}\n", tag.offset);
                }
            }

            ui.output_mut(|o| o.copied_text = taglist);
        }

        egui::ScrollArea::vertical().show_rows(ui, 22.0, self.file_hashes.len(), |ui, range| {
            for tag in &self.file_hashes[range] {
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
