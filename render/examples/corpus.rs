//! Проверка большого набора SVG: конвертирует каждый файл, рисует эталон
//! (resvg) и результат (rlottie), считает разницу и сохраняет худшие случаи.
//!
//! ```sh
//! cargo run --release -p svg2tgs-render --example corpus -- ПАПКА [--out ОТЧЁТ] [--top 20] [--size 256]
//! ```
//!
//! В папке `ОТЧЁТ` (по умолчанию `target/corpus-report`) появятся
//! `summary.txt`, таблица `all.csv` по всем файлам и картинки различий для самых непохожих файлов:
//! эталон | результат | карта различий (красным — заметные отличия).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use svg2tgs_core::{convert, Options};
use svg2tgs_render::{compare, diff_sheet, render_svg, render_tgs, Diff, RLOTTIE_VENDOR};

struct Row {
    path: PathBuf,
    diff: Diff,
    tgs_bytes: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let dir = args
        .iter()
        .find(|a| !a.starts_with("--") && !is_flag_value(&args, a))
        .map(PathBuf::from)
        .expect("укажите папку с SVG");
    let out = PathBuf::from(flag("--out").unwrap_or_else(|| "target/corpus-report".into()));
    let top: usize = flag("--top").map_or(20, |v| v.parse().unwrap());
    let size: u32 = flag("--size").map_or(256, |v| v.parse().unwrap());

    let mut files = Vec::new();
    collect(&dir, &mut files);
    files.sort();
    println!("{} файлов, rlottie: {RLOTTIE_VENDOR}", files.len());

    let options = Options::default();
    let rows = Mutex::new(Vec::new());
    let failed = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());

    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                let svg = match std::fs::read(path) {
                    Ok(b) => b,
                    Err(e) => {
                        failed
                            .lock()
                            .unwrap()
                            .push(format!("{}: {e}", path.display()));
                        continue;
                    }
                };
                let conv = match convert(&svg, &options) {
                    Ok(c) => c,
                    Err(e) => {
                        failed
                            .lock()
                            .unwrap()
                            .push(format!("{}: {e}", path.display()));
                        continue;
                    }
                };
                let (Ok(a), Ok(b)) = (
                    render_svg(&svg, size, &options),
                    render_tgs(&conv.tgs, size, 0),
                ) else {
                    failed
                        .lock()
                        .unwrap()
                        .push(format!("{}: рендер не удался", path.display()));
                    continue;
                };
                rows.lock().unwrap().push(Row {
                    path: path.clone(),
                    diff: compare(&a, &b),
                    tgs_bytes: conv.tgs_bytes(),
                });
            });
        }
    });

    let mut rows = rows.into_inner().unwrap();
    let failed = failed.into_inner().unwrap();
    rows.sort_by(|a, b| {
        b.diff
            .bad_ratio
            .total_cmp(&a.diff.bad_ratio)
            .then(b.diff.mean.total_cmp(&a.diff.mean))
    });

    std::fs::create_dir_all(&out).unwrap();
    let mut summary = String::new();
    let pct = |p: f64| {
        let mut v: Vec<f64> = rows.iter().map(|r| r.diff.mean).collect();
        v.sort_by(f64::total_cmp);
        v.get(((v.len() as f64 - 1.0) * p) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let max_kb = rows.iter().map(|r| r.tgs_bytes).max().unwrap_or(0) as f64 / 1024.0;
    summary += &format!(
        "rlottie: {RLOTTIE_VENDOR}\nфайлов: {}, ошибок: {}\nсредняя разница: медиана {:.2}, 99% {:.2}, максимум {:.2}\nс заметными отличиями (>0.5% пикселей): {}\nсамый большой TGS: {max_kb:.1} КБ\n\nХудшие:\n",
        rows.len(),
        failed.len(),
        pct(0.5),
        pct(0.99),
        pct(1.0),
        rows.iter().filter(|r| r.diff.bad_ratio > 0.005).count(),
    );
    for (n, r) in rows.iter().take(top).enumerate() {
        summary += &format!(
            "{:>3}. {}  разница {:.2}, заметно {:.2}%\n",
            n + 1,
            r.path.display(),
            r.diff.mean,
            r.diff.bad_ratio * 100.0
        );
        let svg = std::fs::read(&r.path).unwrap();
        let conv = convert(&svg, &options).unwrap();
        let a = render_svg(&svg, size, &options).unwrap();
        let b = render_tgs(&conv.tgs, size, 0).unwrap();
        let name = format!(
            "{:03}-{}.png",
            n + 1,
            r.path.file_stem().unwrap().to_string_lossy()
        );
        std::fs::write(out.join(name), diff_sheet(&a, &b).to_png()).unwrap();
    }
    if !failed.is_empty() {
        summary += "\nОшибки:\n";
        for f in &failed {
            summary += &format!("  {f}\n");
        }
    }
    std::fs::write(out.join("summary.txt"), &summary).unwrap();
    let mut csv = String::from("file,mean,bad_ratio,tgs_bytes\n");
    for r in &rows {
        csv += &format!(
            "{},{:.4},{:.6},{}\n",
            r.path.display(),
            r.diff.mean,
            r.diff.bad_ratio,
            r.tgs_bytes
        );
    }
    std::fs::write(out.join("all.csv"), csv).unwrap();
    print!("{summary}");
    println!("\nОтчёт: {}", out.display());
}

fn is_flag_value(args: &[String], a: &String) -> bool {
    args.windows(2)
        .any(|w| w[0].starts_with("--") && &w[1] == a)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("svg")) {
            out.push(p);
        }
    }
}
