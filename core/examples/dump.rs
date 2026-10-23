//! Отладка: конвертирует SVG и печатает замечания, размеры и (по флагу)
//! Lottie JSON. Пример: `cargo run -p svg2tgs-core --example dump -- file.svg --json`.

use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_json = args.iter().any(|a| a == "--json");
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let svg = std::fs::read(path).expect("не удалось прочитать файл");
        match svg2tgs_core::convert(&svg, &svg2tgs_core::Options::default()) {
            Ok(c) => {
                println!("{path}: tgs {} Б, json {} Б", c.tgs_bytes(), c.json_bytes);
                for w in &c.warnings {
                    println!("  ! {}", serde_json::to_string(w).unwrap());
                }
                if show_json {
                    let mut json = String::new();
                    flate2::read::GzDecoder::new(&c.tgs[..])
                        .read_to_string(&mut json)
                        .unwrap();
                    println!("{json}");
                }
            }
            Err(e) => println!("{path}: ошибка: {e:?}"),
        }
    }
}
