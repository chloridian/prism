use gpui_base::{HslaExt as _, RgbaExt as _};
use std::{rc::Rc, sync::OnceLock};

use gpui_base::LogicalStyledExt as _;

use crate::{ActiveTheme, Icon, IconName, InteractiveElementExt as _, Sizable as _, h_flex};
#[cfg(target_os = "linux")]
use gpui::Subscription;
use gpui::{
    AnyElement, App, Background, ClickEvent, ColorExt as _, Context, Decorations, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Render, RenderOnce, Rgba,
    StatefulInteractiveElement as _, StyleRefinement, Styled, TitlebarOptions, Window,
    WindowButton, WindowButtonLayout, WindowControlArea, WindowOptions, div, linear_color_stop,
    linear_gradient, prelude::FluentBuilder as _, px, relative, rems,
};
use gpui_base::TestSupportExt as _;
use smallvec::SmallVec;

pub const TITLE_BAR_HEIGHT: Pixels = px(34.);
const WINDOWS_CAPTION_BUTTON_WIDTH: Pixels = px(46.);
const WINDOWS_CAPTION_BUTTON_GAP: Pixels = px(1.);
const WINDOWS_CAPTION_GLYPH_SIZE: Pixels = px(10.);
const WINDOWS_CAPTION_FONT: &str = "Segoe Fluent Icons";
const WINDOWS_CLOSE_RED: u32 = 0xC42B1C;
#[cfg(target_os = "macos")]
const TITLE_BAR_LEFT_PADDING: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_LEFT_PADDING: Pixels = px(12.);

fn default_title_bar_background(title_bar: Hsla, background: Hsla) -> Background {
    let title_bar_rgb = title_bar.to_rgb();
    let background_rgb = background.to_rgb();
    let mixed = gpui::rgb_to_hsla(Rgba::new(
        title_bar_rgb.r() * 0.55 + background_rgb.r() * 0.45,
        title_bar_rgb.g() * 0.55 + background_rgb.g() * 0.45,
        title_bar_rgb.b() * 0.55 + background_rgb.b() * 0.45,
        title_bar_rgb.a() * 0.55 + background_rgb.a() * 0.45,
    ));

    linear_gradient(
        180.,
        linear_color_stop(mixed, 0.),
        linear_color_stop(title_bar, 1.),
    )
}

/// TitleBar used to customize the appearance of the title bar.
///
/// We can put some elements inside the title bar.
#[derive(IntoElement)]
pub struct TitleBar {
    style: StyleRefinement,
    children: SmallVec<[AnyElement; 1]>,
    on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
}

impl TitleBar {
    /// Create a new TitleBar.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            children: SmallVec::new(),
            on_close_window: None,
        }
    }

    /// Returns the default title bar options for compatible with the [`crate::TitleBar`].
    pub fn title_bar_options() -> TitlebarOptions {
        TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
        }
    }

    /// Returns the default window options for compatible with the [`crate::TitleBar`].
    ///
    /// Use this as the base of the [`WindowOptions`] of any window that renders a
    /// [`crate::TitleBar`], so the title bar owns dragging and double clicking itself:
    ///
    /// ```no_run
    /// # mod gpui_kit { pub use gpui::*; pub extern crate gpui_component as component; }
    /// # use gpui_kit::WindowOptions;
    /// # use gpui_kit::component::TitleBar;
    /// let options = WindowOptions {
    ///     window_min_size: None,
    ///     ..TitleBar::window_options()
    /// };
    /// ```
    pub fn window_options() -> WindowOptions {
        WindowOptions {
            titlebar: Some(Self::title_bar_options()),
            // The title bar draws itself and moves the window via `start_window_move`,
            // so AppKit must not treat it as a system window-move region. Otherwise macOS
            // handles title bar double clicks on its own (in addition to `on_double_click`
            // below) and delays title bar clicks while disambiguating double clicks.
            app_owns_titlebar_drag: true,
            ..Default::default()
        }
    }

    /// Add custom for close window event, default is None, then click X button will call `window.remove_window()`.
    /// Linux only, this will do nothing on other platforms.
    pub fn on_close_window(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if cfg!(target_os = "linux") {
            self.on_close_window = Some(Rc::new(Box::new(f)));
        }
        self
    }
}

#[derive(Clone)]
enum ControlIcon {
    Minimize,
    Restore,
    Maximize,
    Close {
        on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
    },
}

fn window_button_layout(_cx: &App) -> WindowButtonLayout {
    #[cfg(target_os = "linux")]
    {
        _cx.button_layout()
            .unwrap_or_else(WindowButtonLayout::linux_default)
    }

    #[cfg(not(target_os = "linux"))]
    {
        WindowButtonLayout {
            left: [None; gpui::MAX_BUTTONS_PER_SIDE],
            right: [
                Some(WindowButton::Minimize),
                Some(WindowButton::Maximize),
                Some(WindowButton::Close),
            ],
        }
    }
}

fn native_buttons(
    buttons: [Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
    supported: gpui::WindowControls,
) -> SmallVec<[WindowButton; gpui::MAX_BUTTONS_PER_SIDE]> {
    let sizing = supported.minimize || supported.maximize;
    buttons
        .into_iter()
        .flatten()
        .filter(|button| sizing || matches!(button, WindowButton::Close))
        .collect()
}

fn supported_buttons(
    buttons: [Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
    supported: gpui::WindowControls,
) -> SmallVec<[WindowButton; gpui::MAX_BUTTONS_PER_SIDE]> {
    buttons
        .into_iter()
        .flatten()
        .filter(|button| match button {
            WindowButton::Minimize => supported.minimize,
            WindowButton::Maximize => supported.maximize,
            WindowButton::Close => true,
        })
        .collect()
}

impl ControlIcon {
    fn minimize() -> Self {
        Self::Minimize
    }

    fn restore() -> Self {
        Self::Restore
    }

    fn maximize() -> Self {
        Self::Maximize
    }

    fn close(on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>) -> Self {
        Self::Close { on_close_window }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::Minimize => "minimize",
            Self::Restore => "restore",
            Self::Maximize => "maximize",
            Self::Close { .. } => "close",
        }
    }

    fn icon(&self) -> IconName {
        match self {
            Self::Minimize => IconName::WindowMinimize,
            Self::Restore => IconName::WindowRestore,
            Self::Maximize => IconName::WindowMaximize,
            Self::Close { .. } => IconName::WindowClose,
        }
    }

    fn windows_glyph(&self) -> &'static str {
        match self {
            Self::Minimize => "\u{E921}",
            Self::Maximize => "\u{E922}",
            Self::Restore => "\u{E923}",
            Self::Close { .. } => "\u{E8BB}",
        }
    }

    fn windows_hover_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            windows_close_red()
        } else {
            cx.theme().foreground.opacity(0.1)
        }
    }

    fn windows_active_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            windows_close_red().opacity(0.9)
        } else {
            cx.theme().foreground.opacity(0.2)
        }
    }

    fn window_control_area(&self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Restore | Self::Maximize => WindowControlArea::Max,
            Self::Close { .. } => WindowControlArea::Close,
        }
    }

    fn is_close(&self) -> bool {
        matches!(self, Self::Close { .. })
    }

    #[inline]
    fn hover_fg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_foreground
        } else {
            cx.theme().secondary_foreground
        }
    }

    #[inline]
    fn hover_bg(&self, cx: &App) -> Hsla {
        if self.is_close() {
            cx.theme().danger
        } else {
            cx.theme().secondary_hover
        }
    }

    #[inline]
    fn active_bg(&self, cx: &mut App) -> Hsla {
        if self.is_close() {
            cx.theme().danger_active
        } else {
            cx.theme().secondary_active
        }
    }
}

#[derive(IntoElement)]
struct ControlButton {
    icon: ControlIcon,
    enabled: bool,
}

impl RenderOnce for ControlButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            icon: control,
            enabled,
        } = self;
        let is_linux = cfg!(target_os = "linux");
        let is_windows = cfg!(target_os = "windows");
        let windows_glyphs = is_windows && windows_caption_font_available(cx);
        let (hover_fg, hover_bg, active_bg) = if is_windows {
            let fg = if control.is_close() {
                gpui::white()
            } else {
                cx.theme().foreground
            };
            (
                fg,
                control.windows_hover_bg(cx),
                control.windows_active_bg(cx),
            )
        } else {
            (
                control.hover_fg(cx),
                control.hover_bg(cx),
                control.active_bg(cx),
            )
        };
        let foreground = if is_windows && (!enabled || !window.is_window_active()) {
            cx.theme().foreground.opacity(0.4)
        } else {
            cx.theme().foreground
        };
        let id = control.id();
        let glyph = control.windows_glyph();
        let icon_name = control.icon();
        let area = control.window_control_area();
        let on_close_window = match &control {
            ControlIcon::Close { on_close_window } => on_close_window.clone(),
            _ => None,
        };

        div()
            .id(id)
            .test_support()
            .debug_selector(|| id.to_string())
            .flex()
            .w(if is_windows {
                WINDOWS_CAPTION_BUTTON_WIDTH
            } else {
                TITLE_BAR_HEIGHT
            })
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .content_center()
            .items_center()
            .text_color(foreground)
            .when(enabled, |this| {
                this.hover_style(|style| style.bg(hover_bg).text_color(hover_fg))
                    .active_style(|style| style.bg(active_bg).text_color(hover_fg))
            })
            .when(is_windows && enabled, |this| this.window_control_area(area))
            .when(is_linux || !enabled, |this| {
                this.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
            })
            .when(is_linux && enabled, |this| {
                this.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match &control {
                        ControlIcon::Minimize => window.minimize_window(),
                        ControlIcon::Restore | ControlIcon::Maximize => window.zoom_window(),
                        ControlIcon::Close { .. } => {
                            if let Some(f) = on_close_window.clone() {
                                f(&ClickEvent::default(), window, cx);
                            } else {
                                window.remove_window();
                            }
                        }
                    }
                })
            })
            .map(|this| {
                if windows_glyphs {
                    this.child(
                        div()
                            .font_family(WINDOWS_CAPTION_FONT)
                            .text_size(WINDOWS_CAPTION_GLYPH_SIZE)
                            .line_height(relative(1.))
                            .child(glyph),
                    )
                } else {
                    this.child(Icon::new(icon_name).small())
                }
            })
    }
}

fn windows_close_red() -> Hsla {
    gpui::rgb_to_hsla(gpui::rgb(WINDOWS_CLOSE_RED))
}

fn windows_caption_font_available(cx: &App) -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        cx.text_system()
            .all_font_names()
            .iter()
            .any(|name| name == WINDOWS_CAPTION_FONT)
    })
}

#[derive(IntoElement)]
struct WindowControls {
    id: &'static str,
    buttons: [Option<WindowButton>; gpui::MAX_BUTTONS_PER_SIDE],
    on_close_window: Option<Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>>,
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if cfg!(target_os = "macos") || cfg!(target_family = "wasm") {
            return div().id(self.id).test_support();
        }

        #[cfg(target_os = "linux")]
        if !matches!(window.window_decorations(), Decorations::Client { .. }) {
            return div().id(self.id).test_support();
        }

        let supported = window.window_controls();
        let shown = if cfg!(target_os = "linux") {
            supported_buttons(self.buttons, supported)
        } else {
            native_buttons(self.buttons, supported)
        };
        let controls: Vec<ControlButton> = shown
            .into_iter()
            .map(|button| match button {
                WindowButton::Minimize => ControlButton {
                    icon: ControlIcon::minimize(),
                    enabled: supported.minimize,
                },
                WindowButton::Maximize => ControlButton {
                    icon: if window.is_maximized() {
                        ControlIcon::restore()
                    } else {
                        ControlIcon::maximize()
                    },
                    enabled: supported.maximize,
                },
                WindowButton::Close => ControlButton {
                    icon: ControlIcon::close(self.on_close_window.clone()),
                    enabled: true,
                },
            })
            .collect();

        h_flex()
            .flex_row_dir(cx)
            .id(self.id)
            .test_support()
            .items_center()
            .flex_shrink_0()
            .h_full()
            .when(cfg!(target_os = "windows"), |this| this.pb(px(1.)))
            .children(controls.into_iter().enumerate().flat_map(|(ix, control)| {
                let gap = (cfg!(target_os = "windows") && ix > 0).then(caption_gap);
                gap.into_iter()
                    .map(IntoElement::into_any_element)
                    .chain([control.into_any_element()])
            }))
    }
}

fn caption_gap() -> impl IntoElement {
    div()
        .w(WINDOWS_CAPTION_BUTTON_GAP)
        .h_full()
        .flex_shrink_0()
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
}

impl Styled for TitleBar {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for TitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

struct TitleBarState {
    should_move: bool,
    #[cfg(target_os = "linux")]
    // Keep the title bar in sync with system button-layout changes.
    _button_layout_subscription: Subscription,
}

// TODO: Remove this when GPUI has released v0.2.3
impl Render for TitleBarState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl RenderOnce for TitleBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let is_web = cfg!(target_family = "wasm");
        let is_linux = cfg!(target_os = "linux");
        let is_macos = cfg!(target_os = "macos");

        let state = window.use_state(cx, |_window, _state_cx| TitleBarState {
            should_move: false,
            #[cfg(target_os = "linux")]
            _button_layout_subscription: _state_cx
                .observe_button_layout_changed(_window, |_, _, cx| cx.notify()),
        });
        let radii = crate::window_corner_radii(window, cx);
        let button_layout = window_button_layout(cx);

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex_row_dir(cx)
                .when(is_linux, |this| this.flex_row())
                .items_center()
                .justify_between()
                .h(TITLE_BAR_HEIGHT)
                // macOS draws its traffic lights on the left whatever the
                // direction; elsewhere the padding is on the start edge.
                .map(|this| {
                    if is_macos || is_linux {
                        this.pl(TITLE_BAR_LEFT_PADDING)
                    } else {
                        this.ps(TITLE_BAR_LEFT_PADDING, cx)
                    }
                })
                .rounded_tl(radii.top_left)
                .rounded_tr(radii.top_right)
                .border_b_1()
                .border_color(cx.theme().title_bar_border)
                .bg(default_title_bar_background(
                    cx.theme().title_bar,
                    cx.theme().background,
                ))
                .refine_style(&self.style)
                .when(is_linux, |this| {
                    this.on_double_click(|_, window, _| window.zoom_window())
                })
                .when(is_macos, |this| {
                    this.on_double_click(|_, window, _| window.titlebar_double_click())
                })
                // After `on_double_click`, which is implemented for `Stateful` only.
                .test_support()
                .on_mouse_down_out(window.listener_for(&state, |state, _, _, _| {
                    state.should_move = false;
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    // A control that took the press, such as a Button or a
                    // focusable Select, prevents the default; its click must
                    // not turn into a window move on the smallest motion.
                    window.listener_for(&state, |state, _, window, cx| {
                        state.should_move = !window.default_prevented();
                        // The bar moves the window; it never starts a text
                        // selection. On Windows the press arrives through the
                        // caption hit test and the move loop swallows the
                        // release, so a started selection would follow the
                        // pointer afterwards.
                        crate::GlobalState::suppress_text_selection(cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&state, |state, _, _, _| {
                        state.should_move = false;
                    }),
                )
                .on_mouse_move(window.listener_for(
                    &state,
                    |state, event: &gpui::MouseMoveEvent, window, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            state.should_move = false;
                            return;
                        }
                        if state.should_move {
                            state.should_move = false;
                            gpui_base::TextSelection::end(window, cx);
                            cx.stop_active_drag(window);
                            window.start_window_move();
                        }
                    },
                ))
                .when(is_linux, |this| {
                    this.child(WindowControls {
                        id: "window-controls-start",
                        buttons: button_layout.left,
                        on_close_window: self.on_close_window.clone(),
                    })
                })
                .child(
                    h_flex()
                        .id("bar")
                        .h_full()
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .when(!is_web, |this| {
                            this.window_control_area(WindowControlArea::Drag)
                                .when(window.is_fullscreen(), |this| this.ps(rems(0.75), cx))
                                .when(is_linux && is_client_decorated, |this| {
                                    this.child(
                                        div()
                                            .top_0()
                                            .left_0()
                                            .absolute()
                                            .size_full()
                                            .h_full()
                                            .on_mouse_down(
                                                MouseButton::Right,
                                                move |ev, window, _| {
                                                    window.show_window_menu(ev.position)
                                                },
                                            ),
                                    )
                                })
                        })
                        .children(self.children),
                )
                .child(WindowControls {
                    id: "window-controls",
                    buttons: button_layout.right,
                    on_close_window: self.on_close_window,
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        AppContext as _, Modifiers, Rgba, TestAppContext, VisualTestContext, linear_color_stop,
        linear_gradient, point, px,
    };
    use std::cell::Cell;

    use crate::button::Button;

    #[test]
    fn linux_window_controls_follow_layout_order_and_capabilities() {
        let layout = WindowButtonLayout {
            left: [
                Some(WindowButton::Close),
                Some(WindowButton::Minimize),
                None,
            ],
            right: [
                Some(WindowButton::Maximize),
                Some(WindowButton::Close),
                None,
            ],
        };
        let supported = gpui::WindowControls {
            fullscreen: true,
            maximize: false,
            minimize: true,
            window_menu: true,
        };

        assert_eq!(
            supported_buttons(layout.left, supported).as_slice(),
            [WindowButton::Close, WindowButton::Minimize]
        );
        assert_eq!(
            supported_buttons(layout.right, supported).as_slice(),
            [WindowButton::Close]
        );
    }

    #[test]
    fn native_window_controls_dim_one_unsupported_button_and_hide_both() {
        let layout = [
            Some(WindowButton::Minimize),
            Some(WindowButton::Maximize),
            Some(WindowButton::Close),
        ];
        let controls = |minimize, maximize| gpui::WindowControls {
            minimize,
            maximize,
            ..Default::default()
        };

        assert_eq!(native_buttons(layout, controls(false, true)).len(), 3);
        assert_eq!(native_buttons(layout, controls(true, false)).len(), 3);
        assert_eq!(
            native_buttons(layout, controls(false, false)).as_slice(),
            [WindowButton::Close]
        );
    }

    const BUTTON: &str = "title-bar-button";

    struct BarWithButton {
        clicks: Rc<Cell<usize>>,
    }

    impl Render for BarWithButton {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.clicks.clone();
            TitleBar::new().child(div().w(px(200.)).child("App")).child(
                Button::new("open")
                    .label("Open")
                    .debug_selector(|| BUTTON.to_string())
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            )
        }
    }

    fn open(cx: &mut TestAppContext) -> (VisualTestContext, Rc<Cell<usize>>) {
        cx.update(crate::init);
        let clicks = Rc::new(Cell::new(0));
        let view = cx.new(|_| BarWithButton {
            clicks: clicks.clone(),
        });
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                cx.new(|cx| crate::Root::new(view.clone(), window, cx))
            })
            .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (cx, clicks)
    }

    /// Press, move one pixel, release, as a hand often does.
    fn click_with_drift(cx: &mut VisualTestContext, at: gpui::Point<Pixels>) {
        let drift = point(at.x + px(1.), at.y);
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(drift, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(drift, MouseButton::Left, Modifiers::default());
    }

    #[gpui::test]
    fn a_press_on_a_control_keeps_its_click(cx: &mut TestAppContext) {
        let (mut cx, clicks) = open(cx);
        let at = cx
            .debug_bounds(BUTTON)
            .expect("the button is drawn")
            .center();
        click_with_drift(&mut cx, at);
        assert_eq!(clicks.get(), 1);
    }

    #[gpui::test]
    fn a_press_on_the_bar_does_not_start_a_text_selection(cx: &mut TestAppContext) {
        let (mut cx, _) = open(cx);
        let button = cx.debug_bounds(BUTTON).expect("the button is drawn");
        let bar = point(button.left() - px(100.), button.center().y);
        cx.simulate_mouse_down(bar, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| assert!(crate::GlobalState::is_text_selection_suppressed(cx)));
    }

    /// The test platform's `start_window_move` is `unimplemented!()`, so a
    /// press on the bar itself still reaching it shows up as this panic.
    #[gpui::test]
    #[should_panic(expected = "not implemented")]
    fn a_press_on_the_bar_itself_still_moves_the_window(cx: &mut TestAppContext) {
        let (mut cx, _) = open(cx);
        let button = cx.debug_bounds(BUTTON).expect("the button is drawn");
        click_with_drift(&mut cx, point(button.left() - px(100.), button.center().y));
    }

    #[cfg(target_os = "windows")]
    #[gpui::test]
    fn a_press_on_a_caption_button_is_left_to_the_window_frame(cx: &mut TestAppContext) {
        let (mut cx, _) = open(cx);
        for id in ["minimize", "maximize", "close"] {
            let at = cx
                .debug_bounds(id)
                .unwrap_or_else(|| panic!("the {id} button is drawn"))
                .center();
            let result = cx.update(|window, cx| {
                window.dispatch_event(
                    gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                        position: at,
                        modifiers: Modifiers::default(),
                        button: MouseButton::Left,
                        click_count: 1,
                        first_mouse: false,
                    }),
                    cx,
                )
            });
            assert!(result.propagate, "the {id} press was consumed");
        }
    }

    #[cfg(target_os = "windows")]
    struct BorderedBar(f32);

    #[cfg(target_os = "windows")]
    impl Render for BorderedBar {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            TitleBar::new()
                .h(px(40.))
                .border_b(px(self.0))
                .child(div().h_full())
        }
    }

    #[cfg(target_os = "windows")]
    #[gpui::test]
    fn caption_buttons_stop_one_pixel_above_the_bottom_border(cx: &mut TestAppContext) {
        cx.update(crate::init);
        for (border, height) in [(0., 39.), (1., 38.), (2., 37.)] {
            let view = cx.new(|_| BorderedBar(border));
            let window = cx.update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    cx.new(|cx| crate::Root::new(view.clone(), window, cx))
                })
                .unwrap()
            });
            let mut cx = VisualTestContext::from_window(window.into(), cx);
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let close = cx.debug_bounds("close").expect("the close button is drawn");
            assert_eq!(close.top(), px(0.), "border {border}");
            assert_eq!(close.size.height, px(height), "border {border}");
        }
    }

    #[test]
    fn test_default_title_bar_background() {
        let title_bar = gpui::black();
        let background = gpui::white();

        assert_eq!(
            default_title_bar_background(title_bar, background),
            linear_gradient(
                180.,
                linear_color_stop(gpui::rgb_to_hsla(Rgba::new(0.45, 0.45, 0.45, 1.)), 0.,),
                linear_color_stop(title_bar, 1.),
            )
        );
    }
}
