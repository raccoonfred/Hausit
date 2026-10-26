//! Hausit: building information modelling with a Revit-style workflow.
//!
//! Usage: `hausit [PROJECT.hausit | FILE.rvt] [--sample] [--view "View Name"]`

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod browser;
mod canvas;
mod dialogs;
mod icons;
mod options_bar;
mod properties;
mod ribbon;
mod schedule_view;
mod theme;
mod tools;

#[derive(Clone, Default)]
struct Args {
    path: Option<std::path::PathBuf>,
    sample: bool,
    view: Option<String>,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--sample") => args.sample = true,
            Some("--view") => args.view = it.next().map(|v| v.to_string_lossy().to_string()),
            _ => args.path = Some(a.into()),
        }
    }
    args
}

fn run(args: Args, renderer: eframe::Renderer) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hausit")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "Hausit",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            let mut app = app::HausitApp::default();
            if args.sample {
                app.set_document(hausit_model::template::sample_project(), None);
            }
            if let Some(p) = &args.path {
                app.open_path(p);
            }
            if let Some(name) = &args.view {
                let id = app.doc.views().find(|v| &v.name == name).map(|v| v.id);
                if let Some(id) = id {
                    app.open_view(id);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

fn main() -> eframe::Result {
    let args = parse_args();
    // Prefer wgpu (Vulkan/Metal/DX12); fall back to OpenGL on machines
    // without a supported GPU driver.
    match run(args.clone(), eframe::Renderer::Wgpu) {
        Err(e) => {
            eprintln!("wgpu renderer unavailable ({e}); falling back to OpenGL");
            run(args, eframe::Renderer::Glow)
        }
        ok => ok,
    }
}
