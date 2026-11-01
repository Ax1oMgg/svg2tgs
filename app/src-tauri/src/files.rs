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
    /// Заменить существующий файл (пользователь выбрал «Перезаписать»).
    #[serde(default)]
    pub overwrite: bool,
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
    /// Добавить номер: `name_1.tgs`, `name_2.tgs`…
    Number,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub id: String,
    /// `saved`, `skipped` или `failed`.
    pub status: &'static str,
    /// Путь файла (при нарезке — первой клетки).
    pub path: Option<String>,
    /// Сколько файлов записано (клеток нарезки).
    pub count: usize,
}

use svg2tgs_core::naming::safe_file_name as safe_name;
pub use svg2tgs_core::naming::tile_names;

/// Есть ли в папке хоть один из файлов результата.
fn taken(dir: &Path, base: &str, n: usize) -> bool {
    tile_names(base, n)
        .iter()
        .any(|name| dir.join(format!("{name}.tgs")).exists())
}

/// Первое свободное имя `base_1`, `base_2`… в папке `dir` (без расширения):
/// при нарезке свободны должны быть все `n` файлов.
pub fn free_name(dir: &Path, base: &str, n: usize) -> String {
    (1..)
        .map(|k| format!("{base}_{k}"))
        .find(|name| !taken(dir, name, n))
        .expect("свободное имя найдётся")
}

/// Файл, который уже есть там, куда ляжет результат.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Existing {
    pub id: String,
    /// Имя существующего файла с расширением (при нарезке — первой
    /// и последней клетки через многоточие).
    pub file_name: String,
    pub path: String,
    /// Предлагаемое новое имя без расширения: `name_1`.
    pub suggested: String,
}

/// Какие из запросов попадут на существующие файлы; `counts` — сколько
/// файлов у каждого запроса (клеток нарезки).
pub fn find_existing(requests: &[SaveRequest], counts: &[usize]) -> Vec<Existing> {
    requests
        .iter()
        .zip(counts)
        .filter_map(|(r, &n)| {
            let dir = r.folder()?;
            let base = safe_name(&r.name);
            let names = tile_names(&base, n);
            let first = names
                .iter()
                .find(|name| dir.join(format!("{name}.tgs")).exists())?;
            let file_name = if n > 1 {
                format!("{}.tgs … {}.tgs", names[0], names[n - 1])
            } else {
                format!("{base}.tgs")
            };
            Some(Existing {
                id: r.id.clone(),
                file_name,
                path: dir
                    .join(format!("{first}.tgs"))
                    .to_string_lossy()
                    .into_owned(),
                suggested: free_name(&dir, &base, n),
            })
        })
        .collect()
}

pub fn save_all(items: Vec<(SaveRequest, Vec<Vec<u8>>)>, conflict: Conflict) -> Vec<Saved> {
    items
        .into_iter()
        .map(|(req, tiles)| {
            let failed = Saved {
                id: req.id.clone(),
                status: "failed",
                path: None,
                count: 0,
            };
            let Some(dir) = req.folder() else {
                return failed;
            };
            if tiles.is_empty() || std::fs::create_dir_all(&dir).is_err() {
                return failed;
            }
            let n = tiles.len();
            let mut base = safe_name(&req.name);
            if taken(&dir, &base, n) && !req.overwrite {
                match conflict {
                    Conflict::Overwrite => {}
                    Conflict::Skip => {
                        let first = tile_names(&base, n).remove(0);
                        return Saved {
                            id: req.id,
                            status: "skipped",
                            path: Some(
                                dir.join(format!("{first}.tgs"))
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                            count: 0,
                        };
                    }
                    Conflict::Number => base = free_name(&dir, &base, n),
                }
            }
            let paths: Vec<PathBuf> = tile_names(&base, n)
                .iter()
                .map(|name| dir.join(format!("{name}.tgs")))
                .collect();
            for (path, tgs) in paths.iter().zip(&tiles) {
                if std::fs::write(path, tgs).is_err() {
                    return failed;
                }
            }
            Saved {
                id: req.id,
                status: "saved",
                path: Some(paths[0].to_string_lossy().into_owned()),
                count: n,
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
                vec![vec![1, 2, 3]],
            )
        };

        let first = save_all(vec![req("1")], Conflict::Number);
        assert_eq!(first[0].status, "saved");
        let skipped = save_all(vec![req("2")], Conflict::Skip);
        assert_eq!(skipped[0].status, "skipped");
        let numbered = save_all(vec![req("3")], Conflict::Number);
        assert!(numbered[0].path.as_ref().unwrap().ends_with("star_1.tgs"));
        let existing = find_existing(&[req("5").0], &[1]);
        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0].suggested, "star_2");
        let mut forced = req("6");
        forced.0.overwrite = true;
        let replaced = save_all(vec![forced], Conflict::Number);
        assert!(replaced[0].path.as_ref().unwrap().ends_with("star.tgs"));
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
                vec![vec![1]],
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
                Vec::new(),
            )],
            Conflict::Overwrite,
        );
        assert_eq!(out[0].status, "failed");
    }

    #[test]
    fn grid_tiles_are_numbered_and_kept_together() {
        let d = tmp("tiles");
        let dir = d.to_string_lossy().into_owned();
        let req = |id: &str| {
            (
                SaveRequest {
                    id: id.into(),
                    name: "heart".into(),
                    dir: Some(dir.clone()),
                    ..Default::default()
                },
                vec![vec![1]; 12],
            )
        };
        let first = save_all(vec![req("1")], Conflict::Number);
        assert_eq!(first[0].count, 12);
        assert!(d.join("heart_01.tgs").exists() && d.join("heart_12.tgs").exists());
        let existing = find_existing(&[req("2").0], &[12]);
        assert_eq!(existing[0].file_name, "heart_01.tgs … heart_12.tgs");
        assert_eq!(existing[0].suggested, "heart_1");
        let again = save_all(vec![req("3")], Conflict::Number);
        assert!(again[0].path.as_ref().unwrap().ends_with("heart_1_01.tgs"));
    }
}
