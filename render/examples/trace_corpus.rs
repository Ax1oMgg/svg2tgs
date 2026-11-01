//! Прогон трассировки на больших наборах без новых скачиваний: SVG
//! из имеющихся наборов растеризуются resvg, трассируются обратно
//! (`svg2tgs-trace`), результат рисует rlottie и сравнивается с растром.
//! Растровые файлы (PNG, JPEG, WebP) в папке берутся как есть.
//!
//! ```sh
//! cargo run --release -p svg2tgs-render --example trace_corpus -- ПАПКА \
//!     [--out ОТЧЁТ] [--raster 512] [--k 0.5,1,2] [--preset auto] [--grid 8x5] [--top 20] [--size 256]
//! ```
//!
//! `--k` — список значений k (пикселей трассировки на пиксель канваса,
//! `Trace::scale`), по каждому — отдельная строка. В `ОТЧЁТ`
//! (по умолчанию `target/trace-report`): `summary.txt` (по каждому k:
//! доля файлов в лимите, медианы размера и расхождения, время), `all.csv`
//! и картинки худших случаев: растр | результат | карта различий.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use svg2tgs_core::{Grid, Options, Trace, TracePreset, WarningCode};
use svg2tgs_render::{compare, diff_sheet, render_svg, render_tgs, render_tgs_grid, Diff};

struct Row {
    path: PathBuf,
    k: Option<f32>,
    preset: TracePreset,
    traced: u32,
    colors: u32,
    gradients: u32,
    bytes: usize,
    fits: bool,
    diff: Diff,
    ms: u128,
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
        .enumerate()
        .find(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|(_, a)| PathBuf::from(a))
        .expect("укажите папку с SVG или картинками");
    let out = PathBuf::from(flag("--out").unwrap_or_else(|| "target/trace-report".into()));
    let raster: u32 = flag("--raster").map_or(512, |v| v.parse().unwrap());
    let size: u32 = flag("--size").map_or(256, |v| v.parse().unwrap());
    let top: usize = flag("--top").map_or(20, |v| v.parse().unwrap());
    let ks: Vec<Option<f32>> = flag("--k").map_or(vec![None], |v| {
        v.split(',').map(|s| Some(s.parse().unwrap())).collect()
    });
    let preset = match flag("--preset").as_deref() {
        Some("drawing") => TracePreset::Drawing,
        Some("pixel") => TracePreset::Pixel,
        Some("photo") => TracePreset::Photo,
        _ => TracePreset::Auto,
    };
    let grid = flag("--grid").map(|g| {
        let (c, r) = g.split_once('x').unwrap();
        Grid {
            cols: c.parse().unwrap(),
            rows: r.parse().unwrap(),
        }
    });

    let mut files = Vec::new();
    collect(&dir, &mut files);
    files.sort();
    println!("{} файлов, растр {raster} px, k: {ks:?}", files.len());

    let rows = Mutex::new(Vec::new());
    let failed = Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let base = Options {
        grid,
        ..Options::default()
    };
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                let Some(png) = load(path, raster) else {
                    failed.lock().unwrap().push(path.display().to_string());
                    continue;
                };
                for &k in &ks {
                    let options = Options {
                        trace: Trace {
                            preset,
                            scale: k,
                            ..Trace::default()
                        },
                        ..base.clone()
                    };
                    match run(&png, &options, size) {
                        Some((mut row, _, _)) => {
                            row.path = path.clone();
                            row.k = k;
                            rows.lock().unwrap().push(row);
                        }
                        None => failed
                            .lock()
                            .unwrap()
                            .push(format!("{} (k {k:?})", path.display())),
                    }
                }
            });
        }
    });
    let mut rows = rows.into_inner().unwrap();
    let failed = failed.into_inner().unwrap();
    rows.sort_by(|a, b| b.diff.mean.total_cmp(&a.diff.mean));
    std::fs::create_dir_all(&out).unwrap();

    let mut summary = format!(
        "файлов: {}, ошибок: {}, растр {raster} px, пресет {preset:?}, сетка {grid:?}\n\n",
        files.len(),
        failed.len()
    );
    summary += "k        в лимите  размер (медиана, КБ)  разница (медиана / 90%)  время (медиана, мс)  градиентов\n";
    for &k in &ks {
        let r: Vec<&Row> = rows.iter().filter(|r| r.k == k).collect();
        if r.is_empty() {
            continue;
        }
        let med = |mut v: Vec<f64>, p: f64| {
            v.sort_by(f64::total_cmp);
            v[((v.len() - 1) as f64 * p) as usize]
        };
        summary += &format!(
            "{:<8} {:>6.1}%  {:>12.1}  {:>14.2} / {:<6.2}  {:>14.0}  {:>8}\n",
            k.map_or("пресет".into(), |k| k.to_string()),
            r.iter().filter(|r| r.fits).count() as f64 * 100.0 / r.len() as f64,
            med(r.iter().map(|r| r.bytes as f64 / 1024.0).collect(), 0.5),
            med(r.iter().map(|r| r.diff.mean).collect(), 0.5),
            med(r.iter().map(|r| r.diff.mean).collect(), 0.9),
            med(r.iter().map(|r| r.ms as f64).collect(), 0.5),
            r.iter().map(|r| r.gradients as u64).sum::<u64>(),
        );
    }
    summary += "\nХудшие:\n";
    for (n, r) in rows.iter().take(top).enumerate() {
        summary += &format!(
            "{:>3}. {} (k {:?}, {:?}, {} px, {} цв.)  разница {:.2}, {:.1} КБ\n",
            n + 1,
            r.path.display(),
            r.k,
            r.preset,
            r.traced,
            r.colors,
            r.diff.mean,
            r.bytes as f64 / 1024.0
        );
        let png = load(&r.path, raster).unwrap();
        let options = Options {
            trace: Trace {
                preset,
                scale: r.k,
                ..Trace::default()
            },
            ..base.clone()
        };
        if let Some((_, a, b)) = run(&png, &options, size) {
            let stem = format!(
                "{:03}-{}",
                n + 1,
                r.path.file_stem().unwrap().to_string_lossy()
            );
            std::fs::write(out.join(format!("{stem}.png")), diff_sheet(&a, &b).to_png()).unwrap();
            // Растр и SVG трассировки — для разбора.
            std::fs::write(out.join(format!("{stem}.raster.png")), &png).unwrap();
            if let Ok(t) = svg2tgs_trace::convert_grid(&png, &options) {
                std::fs::write(out.join(format!("{stem}.trace.svg")), t.svg).unwrap();
            }
        }
    }
    if !failed.is_empty() {
        summary += "\nОшибки:\n";
        for f in &failed {
            summary += &format!("  {f}\n");
        }
    }
    std::fs::write(out.join("summary.txt"), &summary).unwrap();
    let mut csv =
        String::from("file,k,preset,traced,colors,gradients,tgs_bytes,fits,mean,bad_ratio,ms\n");
    for r in &rows {
        csv += &format!(
            "{},{},{:?},{},{},{},{},{},{:.4},{:.6},{}\n",
            r.path.display(),
            r.k.map_or(String::new(), |k| k.to_string()),
            r.preset,
            r.traced,
            r.colors,
            r.gradients,
            r.bytes,
            r.fits,
            r.diff.mean,
            r.diff.bad_ratio,
            r.ms
        );
    }
    std::fs::write(out.join("all.csv"), csv).unwrap();
    print!("{summary}");
    println!("\nОтчёт: {}", out.display());
}

/// Растр: файл как есть или SVG, нарисованный resvg в `raster` px.
fn load(path: &Path, raster: u32) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if svg2tgs_trace::is_raster(&bytes) {
        return Some(bytes);
    }
    let img = render_svg(&bytes, raster, &Options::default()).ok()?;
    Some(img.to_png())
}

fn run(
    png: &[u8],
    options: &Options,
    size: u32,
) -> Option<(Row, svg2tgs_render::Image, svg2tgs_render::Image)> {
    let t0 = Instant::now();
    let t = svg2tgs_trace::convert_grid(png, options).ok()?;
    let ms = t0.elapsed().as_millis();
    let preview = svg2tgs_trace::preview_svg(png, options).ok()?;
    let a = render_svg(&preview, size, options).ok()?;
    let tiles: Vec<Vec<u8>> = t.tiles.tiles.iter().map(|c| c.tgs.clone()).collect();
    let b = if tiles.len() == 1 {
        render_tgs(&tiles[0], size, 0).ok()?
    } else {
        let side = t.tiles.grid.cols.max(t.tiles.grid.rows);
        render_tgs_grid(&tiles, t.tiles.grid.cols, size / side).ok()?
    };
    let b = if (b.width, b.height) != (a.width, a.height) {
        return None;
    } else {
        b
    };
    let fits = t.tiles.tiles.iter().all(|c| {
        !c.warnings.iter().any(|w| {
            matches!(
                w.code,
                WarningCode::SizeOverLimit { .. } | WarningCode::TooComplex { .. }
            )
        })
    });
    let row = Row {
        path: PathBuf::new(),
        k: None,
        preset: t.info.preset,
        traced: t.info.traced_width.max(t.info.traced_height),
        colors: t.info.colors,
        gradients: t.info.gradient_count,
        bytes: t
            .tiles
            .tiles
            .iter()
            .map(|c| c.tgs_bytes())
            .max()
            .unwrap_or(0),
        fits,
        diff: compare(&a, &b),
        ms,
    };
    Some((row, a, b))
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| {
            ["svg", "png", "jpg", "jpeg", "webp"]
                .iter()
                .any(|e| x.eq_ignore_ascii_case(e))
        }) {
            out.push(p);
        }
    }
}
