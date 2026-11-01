//! Цветные области → контуры → SVG. Перенесено из VTracer
//! (<https://github.com/visioncortex/vtracer>, MIT, `converter.rs`
//! и `svg.rs`): кластеризация visioncortex «стопкой» (крупные области
//! рисуются первыми и заходят под мелкие — без щелей между ними),
//! прозрачность — ключевым цветом. Своё: цвет заранее разбит на палитру
//! (`quantize`), ключевой цвет подбирается без случайности, группы
//! с градиентом получают градиентную заливку.

use std::fmt::Write;

use visioncortex::color_clusters::{KeyingAction, Runner, RunnerConfig, HIERARCHICAL_MAX};
use visioncortex::{Color, ColorImage, PathSimplifyMode, PointF64};

use crate::gradient::{Gradient, Gradients};
#[cfg(test)]
use crate::image::Rgba;
use crate::quantize::{Quantized, CLEAR};

/// Параметры векторизации в пикселях трассировки.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub mode: PathSimplifyMode,
    /// Наименьшее пятно, пикселей (площадь).
    pub speckle_area: usize,
    pub corner_threshold: f64,
    pub length_threshold: f64,
    pub splice_threshold: f64,
}

/// Готовый SVG и сколько градиентов в него попало.
pub struct Svg {
    pub text: String,
    pub gradients: u32,
}

/// Как ложится трассировка на исходную картинку: `viewBox` в единицах
/// трассировки и размер документа в пикселях исходника.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub view: [f64; 4],
    pub width: u32,
    pub height: u32,
}

/// Непрозрачность, ниже которой пиксель вне градиента прозрачен.
pub const OPAQUE: u8 = 128;

/// Ключ прозрачности: цвет, дальше всего отстоящий от цветов рисунка
/// `used` (кластеризация сливает близкие цвета — ключ рядом с настоящим
/// цветом съел бы его). Без случайности: перебор по сетке RGB.
fn key_color(used: &[[u8; 3]]) -> Color {
    let mut best = ([255u8, 0, 255], -1i32);
    for r in (0..=255).step_by(17) {
        for g in (0..=255).step_by(17) {
            for b in (0..=255).step_by(17) {
                let c = [r as u8, g as u8, b as u8];
                let d = used
                    .iter()
                    .map(|u| {
                        (0..3)
                            .map(|i| (u[i] as i32 - c[i] as i32).abs())
                            .sum::<i32>()
                    })
                    .min()
                    .unwrap_or(i32::MAX);
                if d > best.1 {
                    best = (c, d);
                }
            }
        }
    }
    Color::new(best.0[0], best.0[1], best.0[2])
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Векторизует разбитую на цвета картинку `q` размера `w × h`.
pub fn vectorize(
    w: u32,
    h: u32,
    q: &Quantized,
    grads: &Gradients,
    params: Params,
    frame: Frame,
) -> Svg {
    // Картинка для кластеризации: цвета палитры (непрозрачные), группы
    // с градиентом — средним цветом, остальное — ключом прозрачности.
    let mut used: Vec<[u8; 3]> = q.colors.iter().map(|c| [c[0], c[1], c[2]]).collect();
    used.extend(
        grads
            .groups
            .iter()
            .map(|g| [g.mean[0], g.mean[1], g.mean[2]]),
    );
    let key = key_color(&used);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    let mut any = false;
    for (i, &label) in q.labels.iter().enumerate() {
        let g = grads.pixel_group.get(i).copied().unwrap_or(Gradients::NONE);
        let c = if g != Gradients::NONE {
            let m = grads.groups[g as usize].mean;
            [m[0], m[1], m[2], 255]
        } else if label != CLEAR && q.colors[label as usize][3] >= OPAQUE {
            let m = q.colors[label as usize];
            [m[0], m[1], m[2], 255]
        } else {
            [key.r, key.g, key.b, 255]
        };
        any |= c[..3] != [key.r, key.g, key.b];
        pixels.extend_from_slice(&c);
    }
    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}" preserveAspectRatio="none">"#,
        frame.width,
        frame.height,
        num(frame.view[0]),
        num(frame.view[1]),
        num(frame.view[2]),
        num(frame.view[3]),
    );
    if !any {
        out.push_str("</svg>");
        return Svg {
            text: out,
            gradients: 0,
        };
    }
    let image = ColorImage {
        pixels,
        width: w as usize,
        height: h as usize,
    };
    let runner = Runner::new(
        RunnerConfig {
            diagonal: false,
            hierarchical: HIERARCHICAL_MAX,
            batch_size: 25600,
            good_min_area: params.speckle_area,
            good_max_area: (w * h) as usize,
            is_same_color_a: 0,
            is_same_color_b: 1,
            deepen_diff: 16,
            hollow_neighbours: 1,
            key_color: key,
            keying_action: KeyingAction::Discard,
        },
        image,
    );
    let clusters = runner.run();
    let view = clusters.view();

    // Кому принадлежит видимый пиксель: рисуется по порядку, сверху — последний.
    let order: Vec<_> = view.clusters_output.iter().rev().copied().collect();
    let mut owner = vec![u32::MAX; (w * h) as usize];
    for (pos, &ci) in order.iter().enumerate() {
        for &i in view.get_cluster(ci).iter() {
            owner[i as usize] = pos as u32;
        }
    }
    // Градиент у кластера — если больше половины его видимых пикселей
    // из одной группы.
    let mut counts: Vec<std::collections::HashMap<u32, u32>> =
        vec![Default::default(); order.len()];
    let mut visible = vec![0u32; order.len()];
    for (i, &o) in owner.iter().enumerate() {
        if o == u32::MAX {
            continue;
        }
        visible[o as usize] += 1;
        let g = grads.pixel_group.get(i).copied().unwrap_or(Gradients::NONE);
        if g != Gradients::NONE {
            *counts[o as usize].entry(g).or_default() += 1;
        }
    }

    let mut used_groups = std::collections::BTreeSet::new();
    for (pos, &ci) in order.iter().enumerate() {
        let cluster = view.get_cluster(ci);
        let paths = cluster.to_compound_path(
            &view,
            false,
            params.mode,
            params.corner_threshold,
            params.length_threshold,
            10,
            params.splice_threshold,
        );
        let (d, offset) = paths.to_svg_string(true, PointF64::default(), Some(2));
        if d.is_empty() {
            continue;
        }
        let group = counts[pos]
            .iter()
            .max_by_key(|(g, n)| (**n, std::cmp::Reverse(**g)))
            .filter(|(_, n)| **n * 2 > visible[pos])
            .map(|(g, _)| *g);
        let transform = format!("translate({},{})", num(offset.x), num(offset.y));
        match group.map(|g| (g, &grads.groups[g as usize])) {
            Some((_, gr)) if gr.flat => {
                let _ = write!(
                    out,
                    r#"<path d="{d}" fill="{}" transform="{transform}"/>"#,
                    hex(gr.mean)
                );
            }
            Some((g, _)) => {
                used_groups.insert(g);
                let id = format!("g{pos}");
                write_gradient(&mut out, &id, &grads.groups[g as usize], offset);
                let _ = write!(
                    out,
                    r#"<path d="{d}" fill="url(#{id})" transform="{transform}"/>"#
                );
            }
            None => {
                let c = cluster.residue_color();
                let _ = write!(
                    out,
                    r#"<path d="{d}" fill="{}" transform="{transform}"/>"#,
                    hex([c.r, c.g, c.b, 255])
                );
            }
        }
    }
    out.push_str("</svg>");
    Svg {
        text: out,
        gradients: used_groups.len() as u32,
    }
}

fn write_gradient(out: &mut String, id: &str, g: &Gradient, offset: PointF64) {
    // Координаты контура сдвинуты на `offset` — градиент тоже.
    let _ = write!(
        out,
        r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}">"#,
        num(g.x1 as f64 - offset.x),
        num(g.y1 as f64 - offset.y),
        num(g.x2 as f64 - offset.x),
        num(g.y2 as f64 - offset.y),
    );
    for (u, c) in &g.stops {
        let _ = write!(
            out,
            r#"<stop offset="{}" stop-color="{}""#,
            num(*u as f64),
            hex(*c)
        );
        if c[3] < 255 {
            let _ = write!(out, r#" stop-opacity="{}""#, num(c[3] as f64 / 255.0));
        }
        out.push_str("/>");
    }
    out.push_str("</linearGradient>");
}

/// Число в SVG: до трёх знаков, без лишних нулей.
fn num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Пиксели картинки, которые станут видимыми контурами, — для проверок.
#[cfg(test)]
pub fn coverage(img: &Rgba) -> usize {
    img.px.iter().filter(|p| p[3] >= OPAQUE).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantize::quantize;

    #[test]
    fn two_squares_become_two_paths() {
        let mut px = vec![[0, 0, 0, 0]; 40 * 20];
        for y in 0..20 {
            for x in 0..20 {
                px[y * 40 + x] = [255, 0, 0, 255];
                px[y * 40 + 20 + x] = [0, 0, 255, 255];
            }
        }
        // Правый квадрат — с прозрачной половиной.
        for y in 0..20 {
            for x in 30..40 {
                px[y * 40 + x] = [0, 0, 0, 0];
            }
        }
        let img = Rgba::new(40, 20, px);
        assert_eq!(coverage(&img), 600);
        let q = quantize(&img, 8, 16);
        let svg = vectorize(
            40,
            20,
            &q,
            &Gradients::default(),
            Params {
                mode: PathSimplifyMode::Polygon,
                speckle_area: 4,
                corner_threshold: 1.0,
                length_threshold: 4.0,
                splice_threshold: 0.78,
            },
            Frame {
                view: [0.0, 0.0, 40.0, 20.0],
                width: 40,
                height: 20,
            },
        );
        assert!(svg.text.contains("#ff0000"), "{}", svg.text);
        assert!(svg.text.contains("#0000ff"));
        assert_eq!(svg.text.matches("<path").count(), 2);
        assert!(svg.text.contains(r#"viewBox="0 0 40 20""#));
    }

    #[test]
    fn key_is_far_from_picture_colors() {
        let k = key_color(&[[0, 0, 0], [255, 255, 255], [250, 230, 40]]);
        for u in [[0u8, 0, 0], [255, 255, 255], [250, 230, 40]] {
            let d: i32 = (0..3)
                .map(|i| (u[i] as i32 - [k.r, k.g, k.b][i] as i32).abs())
                .sum();
            assert!(d >= 200, "{k:?}");
        }
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0001), "0");
    }
}
