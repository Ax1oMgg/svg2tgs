//! Сборка rlottie из исходников в `vendor/` обычным компилятором C++.
//!
//! Без cmake, без bindgen и без сети: всё нужное лежит в репозитории.
//! Требуется только компилятор C++ (MSVC на Windows, clang на macOS, g++/clang
//! на Linux) — тот же, что уже нужен для сборки Tauri.

use std::path::{Path, PathBuf};

fn main() {
    let vendor = if cfg!(feature = "telegram-rlottie") {
        "rlottie-telegram"
    } else {
        "rlottie-samsung"
    };
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("vendor")
        .join(vendor);
    let src = root.join("src");

    // Пустой config.h: без модулей загрузки картинок (stb вкомпилирован),
    // без потоков и без кэша — для статичных кадров они не нужны.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(
        out.join("config.h"),
        "/* svg2tgs: опции rlottie не включены */\n",
    )
    .unwrap();

    let mut files = Vec::new();
    collect_cpp(&src, &mut files);
    files.sort();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .files(&files)
        .include(&out)
        .include(root.join("inc"))
        .warnings(false)
        // Макросы «собираем саму библиотеку»: RLOTTIE_BUILD — у Samsung,
        // LOT_BUILD — у форка Telegram. Без них на Windows функции объявлены
        // как __declspec(dllimport), и MSVC отказывается их определять
        // (ошибка C2491). Библиотека статическая, экспорт ни на что не влияет.
        .define("RLOTTIE_BUILD", None)
        .define("LOT_BUILD", None)
        .define("LOTTIE_DISABLE_ARM_NEON", None);
    for dir in [
        "vector",
        "vector/freetype",
        "vector/pixman",
        "vector/stb",
        "lottie",
        "lottie/zip",
        "lottie/rapidjson",
        "binding/c",
    ] {
        let d = src.join(dir);
        if d.exists() {
            build.include(d);
        }
    }

    if build.get_compiler().is_like_msvc() {
        // Исключения и RTTI на MSVC оставляем как в cmake-сборке rlottie:
        // стандартная библиотека MSVC без них выдаёт предупреждения C4530.
        build.flag("/std:c++14").flag("/EHsc");
    } else {
        build
            .flag("-std=c++14")
            .flag("-fno-exceptions")
            .flag("-fno-rtti")
            .flag("-fvisibility=hidden");
    }
    build.compile("rlottie");

    // lottieparser.cpp на Windows нормализует пути через PathCanonicalizeW
    // из shlwapi. Приложение подтягивало её через WebView2, а тесты
    // и командная строка без этой строки не линковались (LNK2019).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=shlwapi");
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rustc-env=SVG2TGS_RLOTTIE_VENDOR={vendor}");
}

/// Все .cpp в дереве исходников, кроме привязок для WebAssembly.
fn collect_cpp(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "wasm") {
                continue;
            }
            collect_cpp(&path, out);
        } else if path.extension().is_some_and(|e| e == "cpp") {
            out.push(path);
        }
    }
}
