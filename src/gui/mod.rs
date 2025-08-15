#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "audio")]
mod audio_list;
mod common;
mod external_file;
mod hexview;
mod named_tags;
mod packages;
mod raw_strings;
mod strings;
mod style;
mod tag;
mod texturelist;

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Once, OnceLock};

use eframe::egui::{PointerButton, RichText, TextEdit, Widget};
use eframe::egui_wgpu::RenderState;
use eframe::{
    egui::{self},
    emath::Align2,
    epaint::{Color32, Rounding, Vec2},
};
use egui_notify::Toasts;
use lazy_static::lazy_static;
use log::info;
use notify::Watcher;
use parking_lot::{Mutex, RwLock};
use poll_promise::Promise;
use quicktag_core::util::fnv1;
use quicktag_scanner::context::ScannerContext;
use quicktag_scanner::{ScanStatus, TagCache, load_tag_cache, scanner_progress};
use quicktag_strings::localized::{RawStringHashCache, StringCache, create_stringmap};
use rustc_hash::FxHashSet;
use strings::StringViewVariant;
use tiger_pkg::{TagHash, package_manager};

use self::named_tags::NamedTagView;
use self::packages::PackagesView;
use self::raw_strings::RawStringsView;
use self::strings::StringsView;
use self::tag::TagView;
use self::texturelist::TexturesView;
use crate::gui::external_file::ExternalFileScanView;
use crate::gui::tag::TagHistory;
use crate::texture::cache::TextureCache;

#[derive(PartialEq)]
pub enum Panel {
    Tag,
    NamedTags,
    Packages,
    Textures,
    #[cfg(feature = "audio")]
    Audio,
    Strings,
    ExternalFile,
}

#[derive(PartialEq)]
pub enum StringsPanel {
    Localized,
    Raw,
    Hashes,
}

lazy_static! {
    pub static ref TOASTS: Arc<Mutex<Toasts>> = Arc::new(Mutex::new(Toasts::new()));
    pub static ref CACHE: RwLock<Arc<TagCache>> = RwLock::new(Arc::new(TagCache::default()));
    pub static ref RAW_STRING_HASH_LOOKUP: RwLock<Option<Arc<RawStringHashCache>>> =
        RwLock::new(None);
}

pub fn get_string_for_hash(hash: u32) -> Option<String> {
    let lookup = RAW_STRING_HASH_LOOKUP.read();
    let lookup = lookup.as_ref()?;
    lookup.get(&hash)?.first().cloned().map(|(s, _)| s)
}

pub struct QuickTagApp {
    scanner_context: ScannerContext,
    cache_load: Option<Promise<TagCache>>,
    reload_cache: bool,
    cache: Arc<TagCache>,
    current_wordlist_hash: u64,
    tag_history: Rc<RefCell<TagHistory>>,
    strings: Arc<StringCache>,
    raw_strings: Arc<RawStringHashCache>,

    texture_cache: TextureCache,

    tag_input: String,
    tag_split: bool,
    /// (pkg id, entry index)
    tag_split_input: (String, String),

    open_panel: Panel,
    strings_panel: StringsPanel,

    tag_view: Option<TagView>,
    external_file_view: Option<ExternalFileScanView>,

    named_tags_view: NamedTagView,
    packages_view: PackagesView,
    textures_view: TexturesView,
    #[cfg(feature = "audio")]
    audio_view: audio_list::AudioView,
    strings_view: StringsView,
    raw_strings_view: RawStringsView,
    raw_string_hashes_view: StringsView,

    _schemafile_watcher: notify::RecommendedWatcher,
    schemafile_update_rx: Receiver<Result<notify::Event, notify::Error>>,

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

        let (tx, rx) = std::sync::mpsc::channel();
        let mut schemafile_watcher = notify::recommended_watcher(tx).unwrap();
        if !Path::new("schema.txt").exists() {
            std::fs::File::create("schema.txt").expect("Failed to create schema file");
        }
        schemafile_watcher
            .watch(Path::new("schema.txt"), notify::RecursiveMode::NonRecursive)
            .unwrap();

        quicktag_core::classes::load_schemafile();

        QuickTagApp {
            scanner_context: ScannerContext::create(&package_manager())
                .expect("Failed to create scanner context"),
            cache_load: None,
            reload_cache: true,
            tag_history: Rc::new(RefCell::new(TagHistory::default())),
            cache: Default::default(),
            current_wordlist_hash: 0,
            tag_view: None,
            external_file_view: None,
            tag_input: String::new(),
            tag_split: false,
            tag_split_input: (String::new(), String::new()),

            texture_cache: texture_cache.clone(),

            open_panel: Panel::Tag,
            strings_panel: StringsPanel::Localized,

            named_tags_view: NamedTagView::new(),
            packages_view: PackagesView::new(texture_cache.clone()),
            textures_view: TexturesView::new(texture_cache),
            #[cfg(feature = "audio")]
            audio_view: audio_list::AudioView::new(),
            strings_view: StringsView::new(
                strings.clone(),
                Default::default(),
                StringViewVariant::LocalizedStrings,
            ),
            raw_strings_view: RawStringsView::new(Default::default()),
            raw_string_hashes_view: StringsView::new(
                Arc::new(Default::default()),
                Default::default(),
                StringViewVariant::RawWordlist,
            ),

            strings,
            raw_strings: Default::default(),

            _schemafile_watcher: schemafile_watcher,
            schemafile_update_rx: rx,

            wgpu_state: cc.wgpu_render_state.clone().unwrap(),
        }
    }
}

impl eframe::App for QuickTagApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.reload_cache {
            self.cache_load = Some(Promise::spawn_thread("load_cache", move || {
                load_tag_cache()
            }));
            self.reload_cache = false;
        }

        if self.schemafile_update_rx.try_recv().is_ok() {
            quicktag_core::classes::load_schemafile();
            info!("Reloaded schema file");
        }

        ctx.set_style(style::style());
        let mut is_loading_cache = false;
        if let Some(cache_promise) = self.cache_load.as_ref()
            && cache_promise.poll().is_pending()
        {
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
                    ctx.request_repaint();
                });

            // 

            is_loading_cache = true;
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
            *CACHE.write() = self.cache.clone();

            self.strings_view = StringsView::new(
                self.strings.clone(),
                self.cache.clone(),
                StringViewVariant::LocalizedStrings,
            );
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

            let mut wordlist_hasher = DefaultHasher::new();
            quicktag_strings::wordlist::load_wordlist(|s, h| {
                wordlist_hasher.write(s.as_bytes());
                let entry = new_rsh_cache.entry(h).or_default();
                if entry.iter().any(|(s2, _)| s2 == s) {
                    return;
                }

                entry.push((s.to_string(), true));
            });
            self.current_wordlist_hash = wordlist_hasher.finish();

            let mut filtered_wordlist_hashes: StringCache = Default::default();
            let found_hashes: FxHashSet<u32> = self
                .cache
                .hashes
                .iter()
                .flat_map(|(_, scan)| scan.wordlist_hashes.iter().map(|h| h.hash))
                .collect();
            for hash in found_hashes {
                if let Some(strings) = new_rsh_cache.get(&hash) {
                    filtered_wordlist_hashes
                        .insert(hash, strings.iter().map(|(s, _)| s.clone()).collect());
                }
            }
            // for (tag, _) in self
            //     .cache
            //     .hashes
            //     .iter()
            //     .filter(|(_, scan)| scan.wordlist_hashes.iter().any(|c| c.hash == *hash))
            // {
            //     self.string_selected_entries.push((
            //         *tag,
            //         label,
            //         TagType::from_type_subtype(e.file_type, e.file_subtype),
            //     ));
            // }

            self.raw_string_hashes_view = StringsView::new(
                Arc::new(filtered_wordlist_hashes),
                self.cache.clone(),
                StringViewVariant::RawWordlist,
            );

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
            *RAW_STRING_HASH_LOOKUP.write() = Some(Arc::clone(&self.raw_strings));
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

                        if ui.button("Regenerate Cache").clicked() {
                            self.regenerate_cache();
                            ui.close_menu();
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                        // egui::global_dark_light_mode_switch(ui);
                        if self.current_wordlist_hash != self.cache.wordlist_hash && ui.button(RichText::new("Wordlist changed, click here to regenerate your cache").color(Color32::YELLOW)).clicked() {
                            self.regenerate_cache();
                        }
                    });

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
                            if let Some(t) = package_manager()
                                .lookup
                                .tag64_entries
                                .get(&u64::from_be(hash))
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

                            let hash = TagHash(hash);
                            if hash.is_valid() {
                                hash
                            } else {
                                // Try old format hash
                                let s = TagHash(hash.0.swap_bytes());
                                if s.is_valid() {
                                    TOASTS.lock().warning("Old-style flipped hashes (eg. from Alkahest/Charm) are deprecated.");
                                }
                                s
                            }
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
                    if let Some(external_file_view) = &self.external_file_view {
                        ui.selectable_value(
                            &mut self.open_panel,
                            Panel::ExternalFile,
                            format!("File {}", external_file_view.filename),
                        );
                    }
                });

                ui.separator();

                if self.open_panel == Panel::Strings {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.strings_panel, StringsPanel::Localized, "Localized");
                        ui.selectable_value(&mut self.strings_panel, StringsPanel::Raw, "Raw Strings");
                        ui.selectable_value(&mut self.strings_panel, StringsPanel::Hashes, "Hashes");
                    });
                    ui.separator();
                }

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
                    Panel::Strings => match self.strings_panel {
                        StringsPanel::Localized => self.strings_view.view(ctx, ui),
                        StringsPanel::Raw => self.raw_strings_view.view(ctx, ui),
                        StringsPanel::Hashes => self.raw_string_hashes_view.view(ctx, ui),
                    },
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

        TOASTS.lock().show(ctx);

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
            TOASTS.lock().warning(format!(
                "Could not find tag '{}' ({tag}) in cache\nThis usually means it has no references",
                self.tag_input
            ));
        } else {
            TOASTS
                .lock()
                .error(format!("Could not find tag '{}' ({tag})", self.tag_input));
        }

        if push_history {
            self.tag_history.borrow_mut().push(tag);
        }
    }

    fn regenerate_cache(&mut self) {
        if let Err(e) = std::fs::remove_file(quicktag_scanner::cache_path()) {
            log::error!("Failed to remove cache file: {}", e);
        } else {
            self.tag_view = None;
            self.open_panel = Panel::Tag;

            self.reload_cache = true;
        }
    }
}

pub enum ViewAction {
    OpenTag(TagHash),
}

pub trait View {
    fn view(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> Option<ViewAction>;
}
