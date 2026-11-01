//! Архив ZIP без сжатия — для скачивания нескольких TGS одним файлом.
//!
//! TGS уже сжат gzip, поэтому файлы кладутся как есть (метод «stored»).
//! Имена — в UTF-8 (флаг 11 общего назначения), время — 1 января 1980:
//! одинаковые результаты дают одинаковый архив.

/// Собирает архив из пар «путь внутри архива — содержимое».
pub fn store(files: &[(String, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let crc = crc32(data);
        let offset = out.len() as u32;
        let (size, name_len) = (data.len() as u32, name.len() as u16);
        // Локальный заголовок.
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(20u16.to_le_bytes()); // версия для распаковки
        out.extend(UTF8_FLAG.to_le_bytes());
        out.extend(0u16.to_le_bytes()); // без сжатия
        out.extend(DOS_TIME.to_le_bytes());
        out.extend(DOS_DATE.to_le_bytes());
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(name_len.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend_from_slice(data);
        // Запись центрального каталога.
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes()); // создано: версия 2.0
        central.extend(20u16.to_le_bytes());
        central.extend(UTF8_FLAG.to_le_bytes());
        central.extend(0u16.to_le_bytes());
        central.extend(DOS_TIME.to_le_bytes());
        central.extend(DOS_DATE.to_le_bytes());
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(name_len.to_le_bytes());
        central.extend([0u8; 8]); // доп. поле, комментарий, диск, внутр. атрибуты
        central.extend(0u32.to_le_bytes()); // внешние атрибуты
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let (start, len, count) = (out.len() as u32, central.len() as u32, files.len() as u16);
    out.extend(central);
    // Конец центрального каталога.
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]); // номер диска и диска каталога
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(len.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

const UTF8_FLAG: u16 = 1 << 11;
/// 00:00:00 и 1980-01-01 в формате MS-DOS.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 1 << 5 | 1;

/// CRC-32 (IEEE), как требует ZIP.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// Имена файлов архива по центральному каталогу — для проверок.
pub fn names(zip: &[u8]) -> Vec<String> {
    let u16_at = |i: usize| u16::from_le_bytes([zip[i], zip[i + 1]]) as usize;
    let u32_at =
        |i: usize| u32::from_le_bytes([zip[i], zip[i + 1], zip[i + 2], zip[i + 3]]) as usize;
    let end = zip.len() - 22;
    let (count, mut at) = (u16_at(end + 10), u32_at(end + 16));
    let mut out = Vec::new();
    for _ in 0..count {
        let len = u16_at(at + 28);
        let extra = u16_at(at + 30) + u16_at(at + 32);
        out.push(String::from_utf8_lossy(&zip[at + 46..at + 46 + len]).into_owned());
        at += 46 + len + extra;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_reference() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn archive_lists_files_with_content() {
        let files = vec![
            ("a.tgs".to_string(), &b"hello"[..]),
            ("папка/б.tgs".to_string(), &b""[..]),
        ];
        let zip = store(&files);
        assert_eq!(&zip[..4], b"PK\x03\x04");
        assert_eq!(names(&zip), ["a.tgs", "папка/б.tgs"]);
        // Содержимое первого файла — сразу за локальным заголовком.
        assert_eq!(&zip[30 + 5..30 + 10], b"hello");
        // Пустой архив — только конец каталога.
        assert_eq!(store(&[]).len(), 22);
    }
}
