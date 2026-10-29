//! Prints what Hausit can read from a Revit file:
//! `cargo run -p hausit-rvt --example rvt_inspect -- file.rvt [Class ...]`
//! Naming classes prints their parents and fields from the schema.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: rvt_inspect <file.rvt> [Class ...]");
        std::process::exit(2);
    };
    let info = match hausit_rvt::inspect(std::path::Path::new(path)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    print!("{}", info.summary());
    for d in &info.data_streams {
        println!(
            "  {:32} {:>9} bytes, {:>4} gzip members -> {:>10} bytes",
            d.path, d.stored, d.members, d.inflated
        );
    }
    if let Some(s) = &info.schema {
        for class in &args[1..] {
            match s.class(class) {
                Some(c) => println!("{}: parents {:?}, fields {:?}", c.name, &s.ancestry(class)[1..], c.fields),
                None => println!("{class}: not in the schema"),
            }
        }
    }
}
