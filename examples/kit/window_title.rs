use gpui_component_story::AppTitleBar;
use gpui_kit::component::{
    TITLE_BAR_HEIGHT, TitleBar, h_flex,
    label::Label,
    switch::Switch,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::*;

const HEIGHTS: [f32; 4] = [32., 40., 48., 64.];
const DEFAULT_HEIGHT: usize = 1;

#[derive(Clone, Copy)]
struct Settings {
    height: usize,
    disable_minimize: bool,
    disable_maximize: bool,
    close_only: bool,
}

impl Settings {
    const DEFAULT: Self = Self {
        height: DEFAULT_HEIGHT,
        disable_minimize: false,
        disable_maximize: false,
        close_only: false,
    };

    fn minimizable(self) -> bool {
        !self.disable_minimize && !self.close_only
    }

    fn resizable(self) -> bool {
        !self.disable_maximize && !self.close_only
    }
}

struct Example {
    title_bar: Entity<AppTitleBar>,
    settings: Settings,
}

impl Example {
    fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title_bar = cx.new(|cx| {
            let mut bar = AppTitleBar::new("Window title", window, cx);
            bar.set_height(px(HEIGHTS[settings.height]), cx);
            bar
        });
        Self {
            title_bar,
            settings,
        }
    }

    fn reopen(&self, settings: Settings, window: &mut Window, cx: &mut App) {
        open(settings, Some(window.bounds()), cx);
        window.remove_window();
    }

    fn toggle(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        set: fn(&mut Settings, bool),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Switch::new(id)
            .label(label)
            .checked(checked)
            .on_click(cx.listener(move |this, value: &bool, window, cx| {
                let mut settings = this.settings;
                set(&mut settings, *value);
                this.reopen(settings, window, cx);
            }))
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title_bar = self.title_bar.clone();
        let settings = self.settings;

        v_flex().size_full().child(self.title_bar.clone()).child(
            v_flex()
                .p_5()
                .gap_4()
                .size_full()
                .items_center()
                .justify_center()
                .child(Label::new(format!(
                    "Title bar height: {} px (component default {} px)",
                    HEIGHTS[settings.height],
                    f32::from(TITLE_BAR_HEIGHT)
                )))
                .child(
                    TabBar::new("height")
                        .segmented()
                        .selected_index(settings.height)
                        .on_click(cx.listener(move |this, ix: &usize, _, cx| {
                            this.settings.height = *ix;
                            title_bar.update(cx, |bar, cx| bar.set_height(px(HEIGHTS[*ix]), cx));
                            cx.notify();
                        }))
                        .children(HEIGHTS.map(|h| Tab::new().label(format!("{h} px")))),
                )
                .child(
                    h_flex()
                        .gap_5()
                        .child(self.toggle(
                            "disable-minimize",
                            "Disable minimize",
                            settings.disable_minimize,
                            |settings, on| settings.disable_minimize = on,
                            cx,
                        ))
                        .child(self.toggle(
                            "disable-maximize",
                            "Disable maximize",
                            settings.disable_maximize,
                            |settings, on| settings.disable_maximize = on,
                            cx,
                        ))
                        .child(self.toggle(
                            "close-only",
                            "Close only",
                            settings.close_only,
                            |settings, on| settings.close_only = on,
                            cx,
                        )),
                ),
        )
    }
}

fn open(settings: Settings, bounds: Option<Bounds<Pixels>>, cx: &mut App) {
    let bounds = bounds.unwrap_or_else(|| Bounds::centered(None, size(px(900.), px(480.)), cx));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        is_minimizable: settings.minimizable(),
        is_resizable: settings.resizable(),
        ..TitleBar::window_options()
    };

    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| Example::new(settings, window, cx))
    })
    .expect("Failed to open window");
}

fn main() {
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

    app.run(move |cx| {
        gpui_component_story::init(cx);
        open(Settings::DEFAULT, None, cx);
        cx.activate(true);
    });
}
