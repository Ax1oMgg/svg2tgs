//! Имена файлов результата — общие для программы, командной строки
//! и веб-версии, чтобы нарезка называлась везде одинаково.

/// Имя файла без запрещённых в Windows и macOS символов; пустое — `sticker`.
pub fn safe_file_name(name: &str) -> String {
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

/// Имена файлов результата без расширения: `base` или, при нарезке на
/// `n` клеток, `base_01`, `base_02`… (номер с нулями до общей длины).
pub fn tile_names(base: &str, n: usize) -> Vec<String> {
    if n <= 1 {
        return vec![base.to_string()];
    }
    let digits = n.to_string().len().max(2);
    (1..=n).map(|i| format!("{base}_{i:0digits$}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_names_are_cleaned() {
        assert_eq!(safe_file_name("a/b:c?"), "a_b_c_");
        assert_eq!(safe_file_name("  ..."), "sticker");
        assert_eq!(safe_file_name("иконка"), "иконка");
    }

    #[test]
    fn tiles_are_numbered_with_zeros() {
        assert_eq!(tile_names("a", 1), ["a"]);
        assert_eq!(tile_names("a", 3), ["a_01", "a_02", "a_03"]);
        assert_eq!(tile_names("a", 120)[119], "a_120");
        assert_eq!(tile_names("a", 120)[0], "a_001");
    }
}
