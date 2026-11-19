#![cfg_attr(target_family = "wasm", no_main)]

#[path = "../example_support/assets.rs"]
mod assets;
#[path = "../example_support/fonts.rs"]
mod example_support;

use std::path::PathBuf;

use gpui::{
    App, Bounds, Context, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size, svg,
};
use gpui_platform::application;

struct SvgExample;

impl Render for SvgExample {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .size_full()
            .justify_center()
            .items_center()
            .gap_8()
            .bg(rgb(0xffffff))
            .child(
                svg()
                    .path("svg/dragon.svg")
                    .size_8()
                    .text_color(rgb(0xff0000)),
            )
            .child(
                svg()
                    .path("svg/dragon.svg")
                    .size_8()
                    .text_color(rgb(0x00ff00)),
            )
            .child(
                svg()
                    .path("svg/dragon.svg")
                    .size_8()
                    .text_color(rgb(0x0000ff)),
            )
    }
}

fn run_example() {
    application()
        .with_assets(assets::DirAssets::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gpui/assets"),
        ))
        .run(|cx: &mut App| {
            if !example_support::load_fonts(cx) {
                return;
            }
            let bounds = Bounds::centered(None, size(px(300.0), px(300.0)), cx);
            cx.open_window(
                WindowOptions::new().window_bounds(Some(WindowBounds::Windowed(bounds))),
                |_, cx| cx.new(|_| SvgExample),
            )
            .unwrap();
            cx.activate(true);
        });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_platform::web_init();
    run_example();
}
