//! Правила Telegram для наборов, которые создаёт бот.
//!
//! Источник — Bot API, методы `createNewStickerSet`, `addStickerToSet`,
//! `setStickerSetThumbnail` и объект `InputSticker`
//! (<https://core.telegram.org/bots/api#stickers>); сверено 01.11.2026
//! по тексту документации, который повторяют библиотеки teloxide-core
//! (0.13) и telers. Значения закреплены тестами ниже.

/// Самое длинное имя набора (`name`), символов.
pub const NAME_MAX: usize = 64;
/// Самое длинное название набора (`title`), символов.
pub const TITLE_MAX: usize = 64;
/// Сколько стикеров можно передать сразу в `createNewStickerSet`.
pub const INITIAL_MAX: usize = 50;
/// Сколько эмодзи можно привязать к одному стикеру.
pub const EMOJI_MAX: usize = 20;
/// Предел обычного набора стикеров.
pub const STICKERS_PER_SET: usize = 120;
/// Предел набора эмодзи.
pub const EMOJI_PER_SET: usize = 200;
/// Обложка набора стикеров в формате TGS — не больше 32 КБ
/// (`setStickerSetThumbnail`). У набора эмодзи обложка — один из его эмодзи
/// (`setCustomEmojiStickerSetThumbnail`), отдельного файла нет.
pub const THUMB_TGS_MAX: usize = 32 * 1024;

/// Что создаём: набор стикеров или набор эмодзи.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    Stickers,
    Emoji,
}

impl SetKind {
    /// Значение `sticker_type` в Bot API.
    pub fn api_type(self) -> &'static str {
        match self {
            SetKind::Stickers => "regular",
            SetKind::Emoji => "custom_emoji",
        }
    }

    /// Сколько штук помещается в набор.
    pub fn capacity(self) -> usize {
        match self {
            SetKind::Stickers => STICKERS_PER_SET,
            SetKind::Emoji => EMOJI_PER_SET,
        }
    }

    /// Ссылка, по которой набор добавляют в Telegram.
    pub fn link(self, name: &str) -> String {
        match self {
            SetKind::Stickers => format!("https://t.me/addstickers/{name}"),
            SetKind::Emoji => format!("https://t.me/addemoji/{name}"),
        }
    }
}

/// Окончание имени, обязательное для наборов бота.
pub fn suffix(bot_username: &str) -> String {
    format!("_by_{bot_username}")
}

/// Полное имя набора из начала, которое выбрал пользователь.
pub fn set_name(stem: &str, bot_username: &str) -> String {
    format!("{stem}{}", suffix(bot_username))
}

/// Проверка имени по правилам Telegram: латиница, цифры и `_`, начинается
/// с буквы, без двух `_` подряд, оканчивается на `_by_<бот>` (без учёта
/// регистра), 1–64 символа.
pub fn name_is_valid(name: &str, bot_username: &str) -> bool {
    let len = name.chars().count();
    (1..=NAME_MAX).contains(&len)
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.contains("__")
        && name
            .to_ascii_lowercase()
            .ends_with(&suffix(bot_username).to_ascii_lowercase())
}

/// Название: 1–64 символа, не из одних пробелов.
pub fn title_is_valid(title: &str) -> bool {
    !title.trim().is_empty() && title.chars().count() <= TITLE_MAX
}

/// Начало имени из названия: латиница (кириллица — транслитом), цифры
/// и одиночные `_`; начинается с буквы; укорочено так, чтобы с окончанием
/// бота уложиться в 64 символа. Пустое название даёт `stickers`.
pub fn stem_from_title(title: &str, bot_username: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(|c| c.to_lowercase()) {
        let piece = match c {
            'a'..='z' | '0'..='9' => c.to_string(),
            _ => translit(c)
                .map(str::to_string)
                .unwrap_or_else(|| "_".into()),
        };
        for p in piece.chars() {
            if p == '_' && (out.is_empty() || out.ends_with('_')) {
                continue;
            }
            out.push(p);
        }
    }
    let mut stem = out.trim_end_matches('_').to_string();
    if !stem.starts_with(|c: char| c.is_ascii_alphabetic()) {
        stem = if stem.is_empty() {
            "stickers".into()
        } else {
            format!("s_{stem}")
        };
    }
    let room = NAME_MAX.saturating_sub(suffix(bot_username).len());
    stem.truncate(room);
    stem.trim_end_matches('_').to_string()
}

/// Транслитерация русских и украинских букв (как в загранпаспорте, упрощённо).
fn translit(c: char) -> Option<&'static str> {
    Some(match c {
        'а' => "a",
        'б' => "b",
        'в' => "v",
        'г' => "g",
        'ґ' => "g",
        'д' => "d",
        'е' => "e",
        'ё' => "e",
        'є' => "ie",
        'ж' => "zh",
        'з' => "z",
        'и' => "i",
        'і' => "i",
        'ї' => "i",
        'й' => "i",
        'к' => "k",
        'л' => "l",
        'м' => "m",
        'н' => "n",
        'о' => "o",
        'п' => "p",
        'р' => "r",
        'с' => "s",
        'т' => "t",
        'у' => "u",
        'ф' => "f",
        'х' => "kh",
        'ц' => "ts",
        'ч' => "ch",
        'ш' => "sh",
        'щ' => "shch",
        'ъ' => "ie",
        'ы' => "y",
        'ь' => "",
        'э' => "e",
        'ю' => "iu",
        'я' => "ia",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_follow_bot_api() {
        assert_eq!(NAME_MAX, 64);
        assert_eq!(TITLE_MAX, 64);
        assert_eq!(INITIAL_MAX, 50);
        assert_eq!(EMOJI_MAX, 20);
        assert_eq!(THUMB_TGS_MAX, 32 * 1024);
        assert_eq!(SetKind::Stickers.capacity(), 120);
        assert_eq!(SetKind::Emoji.capacity(), 200);
        assert_eq!(SetKind::Stickers.api_type(), "regular");
        assert_eq!(SetKind::Emoji.api_type(), "custom_emoji");
    }

    #[test]
    fn names_are_checked() {
        let bot = "tea_stickers_bot";
        assert!(name_is_valid("hearts_by_tea_stickers_bot", bot));
        assert!(name_is_valid("Hearts_by_Tea_Stickers_Bot", bot));
        assert!(!name_is_valid("hearts", bot));
        assert!(!name_is_valid("1hearts_by_tea_stickers_bot", bot));
        assert!(!name_is_valid("he__arts_by_tea_stickers_bot", bot));
        assert!(!name_is_valid("сердца_by_tea_stickers_bot", bot));
        let long = format!("{}_by_tea_stickers_bot", "a".repeat(50));
        assert!(!name_is_valid(&long, bot));
    }

    #[test]
    fn stem_is_made_from_title() {
        let bot = "tea_stickers_bot";
        assert_eq!(stem_from_title("Сердечки", bot), "serdechki");
        assert_eq!(stem_from_title("Чайная Аксиома", bot), "chainaia_aksioma");
        assert_eq!(stem_from_title("  Hello, World! ", bot), "hello_world");
        assert_eq!(stem_from_title("2026 год", bot), "s_2026_god");
        assert_eq!(stem_from_title("!!!", bot), "stickers");
        let long = stem_from_title(&"щ".repeat(40), bot);
        let name = set_name(&long, bot);
        assert!(name_is_valid(&name, bot), "{name}");
        for t in [
            "Сердечки",
            "Hello, World!",
            "2026 год",
            "!!!",
            "ёжик в тумане",
        ] {
            assert!(
                name_is_valid(&set_name(&stem_from_title(t, bot), bot), bot),
                "{t}"
            );
        }
    }

    #[test]
    fn titles_are_checked() {
        assert!(title_is_valid("Сердечки"));
        assert!(!title_is_valid("   "));
        assert!(title_is_valid(&"я".repeat(64)));
        assert!(!title_is_valid(&"я".repeat(65)));
    }

    #[test]
    fn links_depend_on_kind() {
        assert_eq!(
            SetKind::Emoji.link("a_by_b"),
            "https://t.me/addemoji/a_by_b"
        );
        assert_eq!(
            SetKind::Stickers.link("a_by_b"),
            "https://t.me/addstickers/a_by_b"
        );
    }
}
