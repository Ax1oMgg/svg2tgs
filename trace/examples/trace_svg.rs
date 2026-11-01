//! Трассировка картинки в SVG — для отладки условий трассировки.
//!
//! ```sh
//! cargo run --release -p svg2tgs-trace --example trace_svg -- КАРТИНКА [ВЫХОД.svg] [--preset drawing|pixel|photo] [--remove-bg] [--no-gradients]
//! ```

use svg2tgs_core::{Options, Trace, TracePreset};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = args.first().expect("укажите картинку");
    let out = args.get(1).filter(|a| !a.starts_with("--"));
    let preset = match args
        .iter()
        .position(|a| a == "--preset")
        .map(|i| args[i + 1].as_str())
    {
        Some("drawing") => TracePreset::Drawing,
        Some("pixel") => TracePreset::Pixel,
        Some("photo") => TracePreset::Photo,
        _ => TracePreset::Auto,
    };
    let options = Options {
        trace: Trace {
            preset,
            remove_bg: args.iter().any(|a| a == "--remove-bg"),
            gradients: args.iter().any(|a| a == "--no-gradients").then_some(false),
            ..Trace::default()
        },
        ..Options::default()
    };
    let bytes = std::fs::read(input).expect("чтение");
    let t = svg2tgs_trace::convert_grid(&bytes, &options).expect("трассировка");
    eprintln!("{:#?}", t.info);
    for w in &t.tiles.tiles[0].warnings {
        eprintln!("{:?}", w.code);
    }
    match out {
        Some(p) => std::fs::write(p, &t.svg).expect("запись"),
        None => println!("{}", t.svg),
    }
}
