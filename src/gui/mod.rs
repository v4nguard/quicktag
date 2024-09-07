#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "audio")]
mod audio_list;
mod common;
mod dxgi;
mod external_file;
mod hexview;
mod named_tags;
mod packages;
mod raw_strings;
mod strings;
mod style;
mod tag;
mod texture;
mod texturelist;

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use destiny_pkg::{GameVersion, TagHash};
use eframe::egui::{PointerButton, TextEdit, Widget};
use eframe::egui_wgpu::RenderState;
use eframe::{
    egui::{self},
    emath::Align2,
    epaint::{Color32, Rounding, Vec2},
};
use egui_notify::Toasts;
use log::info;
use poll_promise::Promise;

use self::named_tags::NamedTagView;
use self::packages::PackagesView;
use self::raw_strings::RawStringsView;
use self::strings::StringsView;
use self::tag::TagView;
use self::texture::TextureCache;
use self::texturelist::TexturesView;
use crate::gui::external_file::ExternalFileScanView;
use crate::gui::tag::TagHistory;
use crate::scanner::{fnv1, ScannerContext};
use crate::text::RawStringHashCache;
use crate::{
    package_manager::package_manager,
    scanner,
    scanner::{load_tag_cache, scanner_progress, ScanStatus, TagCache},
    text::{create_stringmap, StringCache},
};

#[derive(PartialEq)]
pub enum Panel {
    Tag,
    NamedTags,
    Packages,
    Textures,
    #[cfg(feature = "audio")]
    Audio,
    Strings,
    RawStrings,
    ExternalFile,
}

pub struct QuickTagApp {
    scanner_context: ScannerContext,
    cache_load: Option<Promise<TagCache>>,
    cache: Arc<TagCache>,
    tag_history: Rc<RefCell<TagHistory>>,
    strings: Arc<StringCache>,
    raw_strings: Arc<RawStringHashCache>,

    texture_cache: TextureCache,

    tag_input: String,
    tag_split: bool,
    /// (pkg id, entry index)
    tag_split_input: (String, String),

    toasts: Toasts,

    open_panel: Panel,

    tag_view: Option<TagView>,
    external_file_view: Option<ExternalFileScanView>,

    named_tags_view: NamedTagView,
    packages_view: PackagesView,
    textures_view: TexturesView,
    #[cfg(feature = "audio")]
    audio_view: AudioView,
    strings_view: StringsView,
    raw_strings_view: RawStringsView,

    pub wgpu_state: RenderState,
}

impl QuickTagApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Destiny_Keys".into(),
            egui::FontData::from_static(include_bytes!("../../Destiny_Keys.otf")),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(1, "Destiny_Keys".to_owned());

        cc.egui_ctx.set_fonts(fonts);

        let strings = Arc::new(create_stringmap().unwrap());
        let texture_cache = TextureCache::new(cc.wgpu_render_state.clone().unwrap());

        QuickTagApp {
            scanner_context: scanner::create_scanner_context(&package_manager())
                .expect("Failed to create scanner context"),
            cache_load: Some(Promise::spawn_thread("load_cache", move || {
                load_tag_cache()
            })),
            tag_history: Rc::new(RefCell::new(TagHistory::default())),
            cache: Default::default(),
            tag_view: None,
            external_file_view: None,
            tag_input: String::new(),
            tag_split: false,
            tag_split_input: (String::new(), String::new()),

            toasts: Toasts::default(),
            texture_cache: texture_cache.clone(),

            open_panel: Panel::Tag,
            named_tags_view: NamedTagView::new(),
            packages_view: PackagesView::new(texture_cache.clone()),
            textures_view: TexturesView::new(texture_cache),
            #[cfg(feature = "audio")]
            audio_view: audio_list::AudioView::new(),
            strings_view: StringsView::new(strings.clone(), Default::default()),
            raw_strings_view: RawStringsView::new(Default::default()),

            strings,
            raw_strings: Default::default(),
            wgpu_state: cc.wgpu_render_state.clone().unwrap(),
        }
    }
}

impl eframe::App for QuickTagApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_style(style::style());
        let mut is_loading_cache = false;
        if let Some(cache_promise) = self.cache_load.as_ref() {
            if cache_promise.poll().is_pending() {
                {
                    let painter = ctx.layer_painter(egui::LayerId::background());
                    painter.rect_filled(
                        egui::Rect::EVERYTHING,
                        Rounding::default(),
                        Color32::from_black_alpha(127),
                    );
                }
                egui::Window::new("Loading cache")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        let progress = if let ScanStatus::Scanning {
                            current_package,
                            total_packages,
                        } = scanner_progress()
                        {
                            current_package as f32 / total_packages as f32
                        } else {
                            0.9999
                        };

                        ui.add(
                            egui::ProgressBar::new(progress)
                                .animate(true)
                                .text(scanner_progress().to_string()),
                        );
                    });

                // 

                is_loading_cache = true;
            }
        }

        if self
            .cache_load
            .as_ref()
            .map(|v| v.poll().is_ready())
            .unwrap_or_default()
        {
            let c = self.cache_load.take().unwrap();
            let cache = c.try_take().unwrap_or_default();
            self.cache = Arc::new(cache);

            self.strings_view = StringsView::new(self.strings.clone(), self.cache.clone());
            self.raw_strings_view = RawStringsView::new(self.cache.clone());

            let mut new_rsh_cache = RawStringHashCache::default();
            for s in self
                .cache
                .hashes
                .iter()
                .flat_map(|(_, sc)| sc.raw_strings.iter().cloned())
            {
                let h = fnv1(s.as_bytes());
                let entry = new_rsh_cache.entry(h).or_default();
                if entry.iter().any(|(s2, _)| s2 == &s) {
                    continue;
                }

                entry.push((s, false));
            }

            #[cfg(feature = "wordlist")]
            {
                const WORDLIST: &'static str = include_str!("../../wordlist.txt");
                let load_start = Instant::now();
                for s in WORDLIST.lines() {
                    let s = s.to_string();
                    let h = fnv1(s.as_bytes());
                    let entry = new_rsh_cache.entry(h).or_default();
                    if entry.iter().any(|(s2, _)| s2 == &s) {
                        continue;
                    }

                    entry.push((s, true));
                }
                info!(
                    "Loading {} strings from embedded wordlist in {}ms",
                    WORDLIST.lines().count(),
                    load_start.elapsed().as_millis()
                );
            }

            // // Dump all raw strings to a csv file
            // if let Ok(mut f) = std::fs::File::create("raw_strings.csv") {
            //     writeln!(f, "hash|string|is_wordlist").unwrap();
            //     for (hash, strings) in new_rsh_cache.iter() {
            //         for (string, is_wordlist) in strings {
            //             writeln!(f, "{:08X}|{}|{}", hash, string, is_wordlist).unwrap();
            //         }
            //     }
            // }

            self.raw_strings = Arc::new(new_rsh_cache);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!is_loading_cache, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Scan file").clicked() {
                            if let Ok(Some(selected_file)) = native_dialog::FileDialog::new()
                                .add_filter("All files", &["*"])
                                .show_open_single_file()
                            {
                                let filename = selected_file
                                    .file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .to_string();
                                let data = std::fs::read(&selected_file).unwrap();
                                self.external_file_view = Some(ExternalFileScanView::new(
                                    filename,
                                    &self.scanner_context,
                                    &data,
                                ));

                                self.open_panel = Panel::ExternalFile;
                            }

                            ui.close_menu();
                        }
                    });

                    // ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    //     egui::global_dark_light_mode_switch(ui);
                    // });
                });
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Tag:");
                    let mut submitted = false;

                    if self.tag_split {
                        submitted |= TextEdit::singleline(&mut self.tag_split_input.0)
                            .hint_text("PKG ID")
                            .desired_width(64.)
                            .ui(ui)
                            .lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));

                        submitted |= TextEdit::singleline(&mut self.tag_split_input.1)
                            .hint_text("Index")
                            .desired_width(64.)
                            .ui(ui)
                            .lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    } else {
                        submitted |= TextEdit::singleline(&mut self.tag_input)
                            .hint_text("32/64-bit hex tag")
                            .desired_width(128. + 8.)
                            .ui(ui)
                            .lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    }

                    if ui.button("Open").clicked() || submitted {
                        let tag_input_trimmed = self.tag_input.trim();
                        let tag = if self.tag_split {
                            let pkg_id = self.tag_split_input.0.trim();
                            let entry_index = self.tag_split_input.1.trim();

                            if pkg_id.is_empty() || entry_index.is_empty() {
                                TagHash::NONE
                            } else {
                                let pkg_id: u16 =
                                    u16::from_str_radix(pkg_id, 16).unwrap_or_default();
                                let entry_index = str::parse(entry_index).unwrap_or_default();
                                TagHash::new(pkg_id, entry_index)
                            }
                        } else if tag_input_trimmed.len() >= 16 {
                            let hash =
                                u64::from_str_radix(tag_input_trimmed, 16).unwrap_or_default();
                            if let Some(t) = package_manager().hash64_table.get(&u64::from_be(hash))
                            {
                                t.hash32
                            } else {
                                TagHash::NONE
                            }
                        } else if tag_input_trimmed.len() > 8
                            && tag_input_trimmed.chars().all(char::is_numeric)
                        {
                            let hash = tag_input_trimmed.parse().unwrap_or_default();
                            TagHash(hash)
                        } else {
                            let hash =
                                u32::from_str_radix(tag_input_trimmed, 16).unwrap_or_default();
                            TagHash(u32::from_be(hash))
                        };

                        self.open_tag(tag, true);
                    }

                    ui.checkbox(&mut self.tag_split, "Split pkg/entry");
                });

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.open_panel, Panel::Tag, "Tag");
                    ui.selectable_value(&mut self.open_panel, Panel::NamedTags, "Named tags");
                    ui.selectable_value(&mut self.open_panel, Panel::Packages, "Packages");
                    ui.selectable_value(&mut self.open_panel, Panel::Textures, "Textures");
                    #[cfg(feature = "audio")]
                    ui.selectable_value(&mut self.open_panel, Panel::Audio, "Audio");
                    ui.selectable_value(&mut self.open_panel, Panel::Strings, "Strings");
                    ui.selectable_value(&mut self.open_panel, Panel::RawStrings, "Raw Strings");
                    if let Some(external_file_view) = &self.external_file_view {
                        ui.selectable_value(
                            &mut self.open_panel,
                            Panel::ExternalFile,
                            format!("File {}", external_file_view.filename),
                        );
                    }
                });

                ui.separator();

                let action = match self.open_panel {
                    Panel::Tag => {
                        if let Some(tagview) = &mut self.tag_view {
                            tagview.view(ctx, ui)
                        } else {
                            ui.label("No tag loaded");
                            None
                        }
                    }
                    Panel::NamedTags => self.named_tags_view.view(ctx, ui),
                    Panel::Packages => self.packages_view.view(ctx, ui),
                    Panel::Textures => self.textures_view.view(ctx, ui),
                    #[cfg(feature = "audio")]
                    Panel::Audio => self.audio_view.view(ctx, ui),
                    Panel::Strings => self.strings_view.view(ctx, ui),
                    Panel::RawStrings => self.raw_strings_view.view(ctx, ui),
                    Panel::ExternalFile => {
                        if let Some(external_file_view) = &mut self.external_file_view {
                            external_file_view.view(ctx, ui, &self.texture_cache)
                        } else {
                            self.open_panel = Panel::Tag;
                            None
                        }
                    }
                };

                if self.open_panel == Panel::Tag && action.is_none() {
                    if ui.input(|i| i.pointer.button_pressed(PointerButton::Extra1)) {
                        let t = self.tag_history.borrow_mut().back();
                        if let Some(t) = t {
                            self.open_tag(t, false);
                        }
                    }

                    if ui.input(|i| i.pointer.button_pressed(PointerButton::Extra2)) {
                        let t = self.tag_history.borrow_mut().forward();
                        if let Some(t) = t {
                            self.open_tag(t, false);
                        }
                    }
                }

                if let Some(action) = action {
                    match action {
                        ViewAction::OpenTag(t) => self.open_tag(t, true),
                    }
                }
            });
        });

        self.toasts.show(ctx);

        // Redraw the window while we're loading textures. This prevents loading textures from seeming "stuck"
        if self.texture_cache.is_loading_textures() {
            ctx.request_repaint();
        }
    }
}

impl QuickTagApp {
    fn open_tag(&mut self, tag: TagHash, push_history: bool) {
        let new_view = TagView::create(
            self.cache.clone(),
            self.tag_history.clone(),
            self.strings.clone(),
            self.raw_strings.clone(),
            tag,
            self.wgpu_state.clone(),
            self.texture_cache.clone(),
        );
        if new_view.is_some() {
            self.tag_view = new_view;
            self.open_panel = Panel::Tag;
        } else if package_manager().get_entry(tag).is_some() {
            self.toasts.warning(format!(
                "Could not find tag '{}' ({tag}) in cache\nThis usually means it has no references",
                self.tag_input
            ));
        } else {
            self.toasts
                .error(format!("Could not find tag '{}' ({tag})", self.tag_input));
        }

        if push_history {
            self.tag_history.borrow_mut().push(tag);
        }
    }
}

pub enum ViewAction {
    OpenTag(TagHash),
}

pub trait View {
    fn view(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> Option<ViewAction>;
}
