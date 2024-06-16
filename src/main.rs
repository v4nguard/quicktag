mod gui;
mod packages;
mod panic_handler;
mod references;
mod scanner;
mod tagtypes;
mod text;
mod util;

use std::sync::Arc;

use clap::Parser;
use destiny_pkg::{PackageManager, PackageVersion};
use eframe::egui::ViewportBuilder;
use eframe::egui_wgpu::WgpuConfiguration;
use eframe::wgpu;
use env_logger::Env;
use log::info;

use crate::packages::initialize_package_manager;
use crate::references::initialize_reference_names;
use crate::{gui::QuickTagApp, packages::package_manager};

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None, disable_version_flag(true))]
struct Args {
    /// Path to packages directory
    packages_path: String,

    /// Game version for the specified packages directory
    #[arg(short, value_enum)]
    version: PackageVersion,
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

    info!("Initializing package manager");
    let pm = PackageManager::new(args.packages_path, args.version).unwrap();

    initialize_package_manager(pm);

    initialize_reference_names();

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: ViewportBuilder::default().with_icon(
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
        Box::new(|cc| Box::new(QuickTagApp::new(cc, package_manager().version))),
    )
}
