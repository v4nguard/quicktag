mod audio;
mod common;
mod dxgi;
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
use std::rc::Rc;
use std::sync::Arc;

use destiny_pkg::{GameVersion, TagHash};
use eframe::egui::{PointerButton, TextEdit, Widget};
use eframe::egui_wgpu::RenderState;
use eframe::{
    egui::{self},
    emath::Align2,
    epaint::{Color32, Rounding, Vec2},
};
use egui_notify::Toasts;
use poll_promise::Promise;

use crate::gui::tag::TagHistory;
use crate::scanner::fnv1;
use crate::text::RawStringHashCache;
use crate::{
    package_manager::package_manager,
    scanner::{load_tag_cache, scanner_progress, ScanStatus, TagCache},
    text::{create_stringmap, StringCache},
};

use self::named_tags::NamedTagView;
use self::packages::PackagesView;
use self::raw_strings::RawStringsView;
use self::strings::StringsView;
use self::tag::TagView;
use self::texture::TextureCache;
use self::texturelist::TexturesView;

#[derive(PartialEq)]
pub enum Panel {
    Tag,
    NamedTags,
    Packages,
    Textures,
    Strings,
    RawStrings,
}

pub struct QuickTagApp {
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

    named_tags_view: NamedTagView,
    packages_view: PackagesView,
    textures_view: TexturesView,
    strings_view: StringsView,
    raw_strings_view: RawStringsView,

    pub wgpu_state: RenderState,
}

impl QuickTagApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>, version: GameVersion) -> Self {
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
            cache_load: Some(Promise::spawn_thread("load_cache", move || {
                load_tag_cache(version)
            })),
            tag_history: Rc::new(RefCell::new(TagHistory::default())),
            cache: Default::default(),
            tag_view: None,
            tag_input: String::new(),
            tag_split: false,
            tag_split_input: (String::new(), String::new()),

            toasts: Toasts::default(),
            texture_cache: texture_cache.clone(),

            open_panel: Panel::Tag,
            named_tags_view: NamedTagView::new(),
            packages_view: PackagesView::new(texture_cache.clone()),
            textures_view: TexturesView::new(texture_cache),
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
                match new_rsh_cache.entry(h) {
                    std::collections::hash_map::Entry::Occupied(mut o) => {
                        let v = o.get_mut();
                        if v.contains(&s) {
                            continue;
                        }
                        v.push(s);
                    }
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(vec![s]);
                    }
                };
            }

            self.raw_strings = Arc::new(new_rsh_cache);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!is_loading_cache, |ui| {
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
                    ui.selectable_value(&mut self.open_panel, Panel::Strings, "Strings");
                    ui.selectable_value(&mut self.open_panel, Panel::RawStrings, "Raw Strings");
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
                    Panel::Strings => self.strings_view.view(ctx, ui),
                    Panel::RawStrings => self.raw_strings_view.view(ctx, ui),
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
