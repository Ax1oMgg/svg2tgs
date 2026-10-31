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
        WarningCode::CanvasNotAllowed { size, required } => {
            format!("канвас {size}×{size}, а Telegram требует {required}×{required}")
        }
        WarningCode::FpsNotAllowed { fps, required } => {
            format!("{fps} кадров/с, а Telegram требует {required}")
        }
        WarningCode::Simplified { tolerance } => format!(
            "чтобы уложиться в лимит, контуры упрощены (отклонение до {tolerance} px из 512)"
        ),
        WarningCode::TooComplex { points, limit } => format!(
            "слишком сложная фигура ({points} точек при пределе {limit}): \
             Telegram её не нарисует — упростите рисунок"
        ),
        WarningCode::ThinLinesForEmoji { width, recommended } => format!(
            "для эмодзи слишком тонкие линии ({width} px из 512, лучше от {recommended}): \
             в строке текста они почти пропадут"
        ),
        WarningCode::DurationTooLong { secs, max } => {
            format!("длительность {secs:.2} с, а Telegram разрешает не больше {max:.0} с")
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

/// Текст ошибки чтения файла настроек.
pub fn options_error(e: &svg2tgs_core::OptionsError) -> String {
    let at = format!("строка {}, столбец {}", e.line, e.column);
    match e.code {
        "syntax" => format!("не JSON ({at})"),
        "value" => format!("значение не того вида ({at})"),
        "not_object" => "ожидается объект {…} с настройками".into(),
        "newer_format" => "файл от более новой версии svg2tgs — обновите программу".into(),
        other => other.into(),
    }
}
