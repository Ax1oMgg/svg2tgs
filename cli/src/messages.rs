//! Тексты для людей: замечания и ошибки ядра по-русски.

use svg2tgs_core::{Error, Warning, WarningCode};

pub fn warning(w: &Warning) -> String {
    let text = match &w.code {
        WarningCode::TextConvertedToPaths => "текст переведён в кривые".to_string(),
        WarningCode::FilterRemoved { filter } => {
            format!(
                "эффект «{}» не поддерживается в Telegram и убран",
                filter_name(filter)
            )
        }
        WarningCode::MaskRemoved => "маска не поддерживается и убрана".to_string(),
        WarningCode::ImageRemoved => {
            "растровая картинка внутри SVG не поддерживается и убрана".to_string()
        }
        WarningCode::PatternReplaced => "узор (pattern) заменён серым цветом".to_string(),
        WarningCode::BlendModeIgnored { mode } => {
            format!("режим наложения «{mode}» не поддерживается, использован обычный")
        }
        WarningCode::SizeOverLimit { bytes, limit } => format!(
            "файл {:.1} КБ при лимите Telegram {:.0} КБ",
            *bytes as f64 / 1024.0,
            *limit as f64 / 1024.0
        ),
        WarningCode::TextDropped => "текст пропущен: не найден подходящий шрифт".to_string(),
        WarningCode::ClipApproximated => {
            "сложная вложенная обрезка передана приближённо".to_string()
        }
        WarningCode::GradientApproximated => "градиент передан приближённо".to_string(),
        WarningCode::StrokeApproximated => {
            "обводка под неравномерным растяжением передана с одинаковой толщиной".to_string()
        }
    };
    match &w.element_id {
        Some(id) => format!("{text} (элемент «{id}»)"),
        None => text,
    }
}

fn filter_name(code: &str) -> &str {
    match code {
        "blur" => "размытие",
        "drop_shadow" => "тень",
        "offset" => "смещение",
        "color_matrix" => "цветовая матрица",
        "lighting" => "освещение",
        "turbulence" => "шум",
        "morphology" => "утолщение/утоньшение",
        other => other,
    }
}

pub fn error(e: &Error) -> String {
    match e {
        Error::InvalidSvg(_) => "файл повреждён или это не SVG".to_string(),
        Error::Empty => "в SVG нет ничего видимого".to_string(),
        Error::InvalidOption(what) => format!("некорректный параметр: {what}"),
    }
}
