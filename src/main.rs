mod classes;
mod gui;
mod package_manager;
mod panic_handler;
mod scanner;
mod tagtypes;
mod text;
mod texture;
mod util;
mod wordlist;

use std::sync::Arc;

use clap::Parser;
use eframe::egui::ViewportBuilder;
use eframe::egui_wgpu::WgpuConfiguration;
use eframe::wgpu;
use env_logger::Env;
use game_detector::InstalledGame;
use log::info;
use tiger_pkg::{DestinyVersion, GameVersion, PackageManager, Version};

use crate::classes::initialize_reference_names;
use crate::package_manager::initialize_package_manager;
use crate::{gui::QuickTagApp, package_manager::package_manager};

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None, disable_version_flag(true))]
struct Args {
    /// Path to packages directory
    packages_path: Option<String>,

    /// Game version for the specified packages directory
    #[arg(short, value_enum)]
    version: Option<GameVersion>,
}

fn main() -> eframe::Result<()> {
    panic_handler::install_hook(None);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();

    let _rt_guard = rt.enter();

    env_logger::Builder::from_env(
        Env::default().default_filter_or("info,wgpu_core=warn,naga=warn"),
    )
    .init();
    let args = Args::parse();

    let packages_path = if let Some(packages_path) = args.packages_path {
        packages_path
    } else if let Some(path) = find_d2_packages_path() {
        let mut path = std::path::PathBuf::from(path);
        path.push("packages");
        path.to_str().unwrap().to_string()
    } else {
        panic!("Could not find Destiny 2 packages directory");
    };

    info!(
        "Initializing package manager for version {:?} at '{}'",
        args.version, packages_path
    );
    let pm = PackageManager::new(
        packages_path,
        args.version
            .unwrap_or(GameVersion::Destiny(DestinyVersion::Destiny2TheFinalShape)),
        None,
    )
    .unwrap();

    initialize_package_manager(pm);

    initialize_reference_names();

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: ViewportBuilder::default()
            .with_title(format!("Quicktag - {}", package_manager().version.name()))
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../quicktag.png"))
                    .expect("Failed to load icon"),
            ),
        persist_window: true,
        follow_system_theme: false,
        default_theme: eframe::Theme::Dark,
        wgpu_options: WgpuConfiguration {
            supported_backends: wgpu::Backends::PRIMARY,
            device_descriptor: Arc::new(|_adapter| wgpu::DeviceDescriptor {
                required_features: wgpu::Features::TEXTURE_COMPRESSION_BC
                    | wgpu::Features::TEXTURE_BINDING_ARRAY
                    | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Quicktag",
        native_options,
        Box::new(|cc| Ok(Box::new(QuickTagApp::new(cc)))),
    )
}

fn find_d2_packages_path() -> Option<String> {
    let mut installations = game_detector::find_all_games();
    installations.retain(|i| match i {
        InstalledGame::Steam(a) => a.appid == 1085660,
        InstalledGame::EpicGames(m) => m.display_name == "Destiny 2",
        InstalledGame::MicrosoftStore(p) => p.app_name == "Destiny2PCbasegame",
        _ => false,
    });

    info!("Found {} Destiny 2 installations", installations.len());

    // Sort installations, weighting Steam > Epic > Microsoft Store
    installations.sort_by_cached_key(|i| match i {
        InstalledGame::Steam(_) => 0,
        InstalledGame::EpicGames(_) => 1,
        InstalledGame::MicrosoftStore(_) => 2,
        _ => 3,
    });

    match installations.first() {
        Some(InstalledGame::Steam(a)) => Some(a.game_path.clone()),
        Some(InstalledGame::EpicGames(m)) => Some(m.install_location.clone()),
        Some(InstalledGame::MicrosoftStore(p)) => Some(p.path.clone()),
        _ => None,
    }
}
