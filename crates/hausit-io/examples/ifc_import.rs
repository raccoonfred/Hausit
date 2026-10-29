//! Converts an IFC file to a Hausit project and prints the import report:
//! `cargo run -p hausit-io --example ifc_import -- model.ifc [out.hausit]`.

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(input) = args.first() else {
        eprintln!("usage: ifc_import <model.ifc> [out.hausit]");
        std::process::exit(2);
    };
    match hausit_io::import_ifc(Path::new(input)) {
        Ok((doc, report)) => {
            println!("{}", report.summary());
            println!("{report:#?}");
            if let Some(out) = args.get(1) {
                if let Err(e) = hausit_io::save(&doc, Path::new(out)) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
