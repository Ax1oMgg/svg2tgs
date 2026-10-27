//! Запуск исполняемого `svg2tgs` так, как это делает пользователь.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_svg2tgs"))
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures")
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("svg2tgs-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn json(out: &std::process::Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&out.stdout).expect("stdout — JSON-отчёт")
}

#[test]
fn folder_with_json_report() {
    let out_dir = tmp("json");
    let out = bin()
        .arg(fixtures())
        .arg("-o")
        .arg(&out_dir)
        .arg("--json")
        .output()
        .unwrap();
    // В папке есть заведомо битые invalid.svg и empty.svg.
    assert_eq!(out.status.code(), Some(2));
    let report = json(&out);
    assert!(report.len() >= 19);
    let by_name = |n: &str| {
        report
            .iter()
            .find(|r| r["input"].as_str().unwrap().ends_with(n))
            .unwrap()
            .clone()
    };
    assert_eq!(by_name("invalid.svg")["error"], "invalid_svg");
    assert_eq!(by_name("empty.svg")["error"], "empty");
    let blur = by_name("filter-blur.svg");
    assert_eq!(blur["status"], "warning");
    assert_eq!(blur["warnings"][0]["code"], "filter_removed");
    let ok = by_name("stroke-icon.svg");
    assert_eq!(ok["status"], "ok");
    assert!(Path::new(ok["output"].as_str().unwrap()).exists());
}

#[test]
fn same_names_in_subfolders_do_not_overwrite_each_other() {
    let src = tmp("dup-src");
    let out_dir = tmp("dup-out");
    for sub in ["a", "b"] {
        std::fs::create_dir_all(src.join(sub)).unwrap();
        std::fs::copy(
            fixtures().join("shapes-basic.svg"),
            src.join(sub).join("icon.svg"),
        )
        .unwrap();
    }
    let out = bin()
        .arg(&src)
        .arg("-r")
        .arg("-o")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out_dir.join("icon.tgs").exists());
    assert!(out_dir.join("icon_1.tgs").exists());
}

#[test]
fn skip_keeps_existing_files() {
    let out_dir = tmp("skip");
    let file = fixtures().join("evenodd-ring.svg");
    let first = bin().arg(&file).arg("-o").arg(&out_dir).output().unwrap();
    assert!(first.status.success());
    let second = bin()
        .arg(&file)
        .arg("-o")
        .arg(&out_dir)
        .args(["--on-conflict", "skip", "--json"])
        .output()
        .unwrap();
    assert!(second.status.success());
    assert_eq!(json(&second)[0]["status"], "skipped");
    assert!(!out_dir.join("evenodd-ring (2).tgs").exists());
}

#[test]
fn options_reach_the_converter() {
    let out_dir = tmp("opts");
    let out = bin()
        .arg(fixtures().join("shapes-basic.svg"))
        .arg("-o")
        .arg(&out_dir)
        .args(["--fps", "30", "--json"])
        .output()
        .unwrap();
    let report = json(&out);
    assert_eq!(report[0]["warnings"][0]["code"], "fps_not_allowed");
}

#[test]
fn bad_arguments_are_explained() {
    let out = bin()
        .arg("x.svg")
        .args(["--padding", "50"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("от 0 до 20"));

    let out = bin().arg(tmp("empty-dir")).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("не найдены"));
}
