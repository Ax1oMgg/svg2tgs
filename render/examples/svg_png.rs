//! SVG → PNG через resvg (как превью исходника): для разбора случаев.
//! `cargo run -p svg2tgs-render --example svg_png -- ВХОД.svg ВЫХОД.png [512]`
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let svg = std::fs::read(&a[0]).unwrap();
    let size = a.get(2).map_or(512, |s| s.parse().unwrap());
    let img = svg2tgs_render::render_svg(&svg, size, &svg2tgs_core::Options::default()).unwrap();
    std::fs::write(&a[1], img.to_png()).unwrap();
}
