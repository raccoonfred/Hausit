//! Hausit: building information modelling with a Revit-style workflow.
//!
//! Usage: `hausit [PROJECT.hausit | FILE.rvt] [--sample | --sample-industrial] [--view "View Name"]
//! [--export-pdf OUT.pdf] [--export-ifc OUT.ifc]`
//!
//! `--export-pdf` writes every sheet of the project to a PDF and exits;
//! `--export-ifc` writes the model as IFC4 and exits.

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
mod sheets;
mod theme;
mod tools;
mod viewcube;

#[derive(Clone, Default)]
struct Args {
    path: Option<std::path::PathBuf>,
    /// Sample project to open: the house, or the industrial shed.
    sample: Option<bool>,
    view: Option<String>,
    /// Export every sheet to this PDF and exit, without a window.
    export_pdf: Option<std::path::PathBuf>,
    /// Export the model to this IFC file and exit.
    export_ifc: Option<std::path::PathBuf>,
}

fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--sample") => args.sample = Some(false),
            Some("--sample-industrial") => args.sample = Some(true),
            Some("--view") => args.view = it.next().map(|v| v.to_string_lossy().to_string()),
            Some("--export-pdf") => args.export_pdf = it.next().map(Into::into),
            Some("--export-ifc") => args.export_ifc = it.next().map(Into::into),
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
            match args.sample {
                Some(false) => app.set_document(sheets::sample_house(), None),
                Some(true) => app.set_document(sheets::sample_warehouse(), None),
                None => {}
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
    if let Some(out) = &args.export_pdf {
        std::process::exit(export_pdf(&args, out));
    }
    if let Some(out) = &args.export_ifc {
        std::process::exit(export_ifc(&args, out));
    }
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

/// The project named on the command line, or a sample.
fn load_for_export(args: &Args) -> Option<hausit_model::Document> {
    match (&args.path, args.sample) {
        (Some(p), _) => match hausit_io::open(p) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("{}: {e}", p.display());
                None
            }
        },
        (None, Some(true)) => Some(sheets::sample_warehouse()),
        _ => Some(sheets::sample_house()),
    }
}

/// `--export-ifc`: the whole model.
fn export_ifc(args: &Args, out: &std::path::Path) -> i32 {
    let Some(doc) = load_for_export(args) else { return 1 };
    match hausit_io::export_ifc(&doc, out) {
        Ok(()) => {
            println!("Wrote {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("{}: {e}", out.display());
            1
        }
    }
}

/// `--export-pdf`: every sheet of the project, in number order.
fn export_pdf(args: &Args, out: &std::path::Path) -> i32 {
    let Some(doc) = load_for_export(args) else { return 1 };
    let ids: Vec<_> = sheets::sheets_in_order(&doc).into_iter().map(|(id, _)| id).collect();
    if ids.is_empty() {
        eprintln!("The project has no sheets.");
        return 1;
    }
    match sheets::write_pdf(&doc, &ids, out) {
        Ok(()) => {
            println!("Wrote {} sheet(s) to {}", ids.len(), out.display());
            0
        }
        Err(e) => {
            eprintln!("{}: {e}", out.display());
            1
        }
    }
}
