//! Работа с файлами: разворачивание папок и сохранение результатов.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Найденный SVG-файл.
#[derive(Serialize, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub name: String,
}

fn is_svg(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"))
}

fn collect(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if recursive {
                collect(&p, recursive, out);
            }
        } else if is_svg(&p) {
            out.push(p);
        }
    }
}

/// Папки разворачиваются в SVG внутри; файлы не-SVG отбрасываются;
/// повторы убираются.
pub fn expand(paths: &[String], recursive: bool) -> Vec<Entry> {
    let mut found = Vec::new();
    for p in paths {
        let p = PathBuf::from(p);
        if p.is_dir() {
            collect(&p, recursive, &mut found);
        } else if is_svg(&p) {
            found.push(p);
        }
    }
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .map(|p| Entry {
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: p.to_string_lossy().into_owned(),
        })
        .collect()
}

/// Что сохранить: файл `id` под именем `name` (без расширения).
///
/// Папка — `dir`, а если она не задана — папка исходного файла `source`.
/// `subfolder` создаётся внутри неё.
#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaveRequest {
    pub id: String,
    pub name: String,
    pub dir: Option<String>,
    pub source: Option<String>,
    pub subfolder: Option<String>,
}

impl SaveRequest {
    /// Папка, куда ляжет файл; `None` — её не из чего взять
    /// (SVG из буфера без выбранной папки).
    pub fn folder(&self) -> Option<PathBuf> {
        let base = match (&self.dir, &self.source) {
            (Some(d), _) if !d.trim().is_empty() => PathBuf::from(d),
            (_, Some(s)) => PathBuf::from(s).parent()?.to_path_buf(),
            _ => return None,
        };
        Some(match self.subfolder.as_deref().map(str::trim) {
            Some(sub) if !sub.is_empty() => base.join(safe_name(sub)),
            _ => base,
        })
    }
}

/// Что делать, если файл с таким именем уже есть.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Conflict {
    Overwrite,
    Skip,
    /// Добавить номер: `name (2).tgs`.
    Number,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub id: String,
    /// `saved`, `skipped` или `failed`.
    pub status: &'static str,
    pub path: Option<String>,
}

/// Имя файла без запрещённых в Windows и macOS символов.
fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        "sticker".into()
    } else {
        trimmed
    }
}

pub fn save_all(items: Vec<(SaveRequest, Option<Vec<u8>>)>, conflict: Conflict) -> Vec<Saved> {
    items
        .into_iter()
        .map(|(req, tgs)| {
            let failed = Saved {
                id: req.id.clone(),
                status: "failed",
                path: None,
            };
            let Some(dir) = req.folder() else {
                return failed;
            };
            let (Some(tgs), true) = (tgs, std::fs::create_dir_all(&dir).is_ok()) else {
                return failed;
            };
            let base = safe_name(&req.name);
            let mut path = dir.join(format!("{base}.tgs"));
            if path.exists() {
                match conflict {
                    Conflict::Overwrite => {}
                    Conflict::Skip => {
                        return Saved {
                            id: req.id,
                            status: "skipped",
                            path: Some(path.to_string_lossy().into_owned()),
                        }
                    }
                    Conflict::Number => {
                        let mut n = 2;
                        while path.exists() {
                            path = dir.join(format!("{base} ({n}).tgs"));
                            n += 1;
                        }
                    }
                }
            }
            match std::fs::write(&path, tgs) {
                Ok(()) => Saved {
                    id: req.id,
                    status: "saved",
                    path: Some(path.to_string_lossy().into_owned()),
                },
                Err(_) => failed,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("svg2tgs-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn folders_expand_to_svgs() {
        let d = tmp("expand");
        std::fs::write(d.join("b.svg"), "x").unwrap();
        std::fs::write(d.join("a.SVG"), "x").unwrap();
        std::fs::write(d.join("note.txt"), "x").unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/c.svg"), "x").unwrap();
        let p = d.to_string_lossy().into_owned();

        let flat: Vec<String> = expand(std::slice::from_ref(&p), false)
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(flat, ["a.SVG", "b.svg"]);
        let deep: Vec<String> = expand(&[p.clone(), p], true)
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(deep, ["a.SVG", "b.svg", "c.svg"], "без повторов");
    }

    #[test]
    fn conflicts_are_resolved() {
        let d = tmp("save");
        let dir = d.to_string_lossy().into_owned();
        let req = |id: &str| {
            (
                SaveRequest {
                    id: id.into(),
                    name: "star".into(),
                    dir: Some(dir.clone()),
                    ..Default::default()
                },
                Some(vec![1, 2, 3]),
            )
        };

        let first = save_all(vec![req("1")], Conflict::Number);
        assert_eq!(first[0].status, "saved");
        let skipped = save_all(vec![req("2")], Conflict::Skip);
        assert_eq!(skipped[0].status, "skipped");
        let numbered = save_all(vec![req("3")], Conflict::Number);
        assert!(numbered[0].path.as_ref().unwrap().ends_with("star (2).tgs"));
        let over = save_all(vec![req("4")], Conflict::Overwrite);
        assert!(over[0].path.as_ref().unwrap().ends_with("star.tgs"));
    }

    #[test]
    fn saves_next_to_source_in_subfolder() {
        let d = tmp("beside");
        let source = d.join("art").join("heart.svg");
        let out = save_all(
            vec![(
                SaveRequest {
                    id: "1".into(),
                    name: "heart_tg".into(),
                    source: Some(source.to_string_lossy().into_owned()),
                    subfolder: Some("tgs".into()),
                    ..Default::default()
                },
                Some(vec![1]),
            )],
            Conflict::Number,
        );
        assert_eq!(out[0].status, "saved");
        assert!(d.join("art/tgs/heart_tg.tgs").exists());

        // Без папки и без исходного файла (SVG из буфера) сохранить некуда.
        let nowhere = SaveRequest {
            id: "2".into(),
            name: "x".into(),
            ..Default::default()
        };
        assert_eq!(nowhere.folder(), None);
    }

    #[test]
    fn unsafe_names_are_cleaned() {
        assert_eq!(safe_name("a/b:c?"), "a_b_c_");
        assert_eq!(safe_name("  ..."), "sticker");
        assert_eq!(safe_name("иконка"), "иконка");
    }

    #[test]
    fn missing_result_is_failed() {
        let d = tmp("missing");
        let out = save_all(
            vec![(
                SaveRequest {
                    id: "x".into(),
                    name: "x".into(),
                    dir: Some(d.to_string_lossy().into_owned()),
                    ..Default::default()
                },
                None,
            )],
            Conflict::Overwrite,
        );
        assert_eq!(out[0].status, "failed");
    }
}
