// Без окна консоли в релизной сборке Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    svg2tgs_app_lib::run()
}
