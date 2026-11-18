//! A GPUI window on a winit window: event translation, drawing and the `PlatformWindow` implementation.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
    rc::Rc,
    sync::Arc,
    time::Instant,
};

use anyhow::{Context as _, Result, bail};
use futures::channel::oneshot;
use gpui::{
    AnyWindowHandle, Bounds, Capslock, DevicePixels, DispatchEventResult, GpuSpecs, KeyDownEvent,
    KeyUpEvent, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformAtlas, PlatformDisplay, PlatformInput,
    PlatformInputHandler, PlatformWindow, Point, PromptButton, PromptLevel, RawMouseMotionEvent,
    RequestFrameOptions, ResizeEdge, Scene, ScrollDelta, ScrollWheelEvent, Size, TouchPhase,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowKind,
    WindowParams, point, px, size,
};
use gpui_util::ResultExt as _;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle, WindowHandle,
};
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ScreenToClient,
    System::Diagnostics::Debug::MessageBeep,
    UI::WindowsAndMessaging::{
        GetCursorPos, MB_OK, SPI_GETWHEELSCROLLCHARS, SPI_GETWHEELSCROLLLINES,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    },
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    platform::windows::{BackdropType, WindowAttributesExtWindows as _, WindowExtWindows as _},
    window::{
        CursorGrabMode, Fullscreen, ResizeDirection, Theme, Window, WindowAttributes, WindowId,
        WindowLevel,
    },
};

use crate::{
    capture::{Capture, Grab},
    display::{WinitDisplay, logical_point},
    input::{self, ClickState, DoubleClick},
    renderer::{Renderer, RendererContext},
    system,
};

pub(crate) type Windows = Rc<RefCell<HashMap<WindowId, Rc<WindowInner>>>>;

/// The cursor appearance every window shows.
pub(crate) struct CursorState {
    pub(crate) style: Cell<gpui::CursorStyle>,
    pub(crate) visible: Cell<bool>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            style: Cell::new(gpui::CursorStyle::Arrow),
            visible: Cell::new(true),
        }
    }
}

type Slot<F> = Cell<Option<Box<F>>>;

#[derive(Default)]
struct Callbacks {
    request_frame: Slot<dyn FnMut(RequestFrameOptions)>,
    input: Slot<dyn FnMut(PlatformInput) -> DispatchEventResult>,
    active_status_change: Slot<dyn FnMut(bool)>,
    hovered_status_change: Slot<dyn FnMut(bool)>,
    resize: Slot<dyn FnMut(Size<Pixels>, f32)>,
    moved: Slot<dyn FnMut()>,
    should_close: Slot<dyn FnMut() -> bool>,
    close: Slot<dyn FnOnce()>,
    hit_test_window_control: Slot<dyn FnMut() -> Option<WindowControlArea>>,
    appearance_changed: Slot<dyn FnMut()>,
}

pub(crate) struct WindowInner {
    // The renderer owns the surface, so it must drop before the window it draws to.
    renderer: RefCell<Renderer>,
    window: Arc<Window>,
    handle: AnyWindowHandle,
    hwnd: HWND,
    cursor: Rc<CursorState>,
    capture: RefCell<Capture>,
    callbacks: Callbacks,
    input_handler: Cell<Option<PlatformInputHandler>>,
    scale_factor: Cell<f32>,
    modifiers: Cell<Modifiers>,
    mouse_position: Cell<Point<Pixels>>,
    pressed_button: Cell<Option<MouseButton>>,
    click_state: RefCell<ClickState>,
    hovered: Cell<bool>,
    active: Cell<bool>,
    appearance: Cell<WindowAppearance>,
    background_appearance: Cell<WindowBackgroundAppearance>,
    restore_bounds: Cell<Bounds<Pixels>>,
    frame_requested: Cell<bool>,
    force_render_pending: Cell<bool>,
}

impl WindowInner {
    pub(crate) fn open(
        event_loop: &ActiveEventLoop,
        handle: AnyWindowHandle,
        params: WindowParams,
        renderer_context: RendererContext,
        cursor: Rc<CursorState>,
    ) -> Result<Rc<Self>> {
        if let WindowKind::AnchoredPopup(_) = params.kind {
            return Err(gpui::popup::PopupNotSupportedError.into());
        }

        let display = params
            .display_id
            .and_then(WinitDisplay::new)
            .or_else(WinitDisplay::primary)
            .context("no display to open the window on")?;
        let scale_factor = display.scale_factor();
        let bounds = params.bounds;
        let title_bar = params.titlebar.as_ref();

        let mut attributes = WindowAttributes::default()
            .with_title(
                title_bar
                    .and_then(|titlebar| titlebar.title.as_ref())
                    .map_or("", |title| title.as_ref()),
            )
            .with_visible(false)
            .with_resizable(params.is_resizable)
            .with_inner_size(PhysicalSize::new(
                physical(bounds.size.width, scale_factor),
                physical(bounds.size.height, scale_factor),
            ))
            .with_position(PhysicalPosition::new(
                (bounds.origin.x.as_f32() * scale_factor) as i32,
                (bounds.origin.y.as_f32() * scale_factor) as i32,
            ))
            .with_decorations(!title_bar.is_some_and(|titlebar| titlebar.appears_transparent))
            .with_undecorated_shadow(true);
        if let Some(min_size) = params.window_min_size {
            attributes = attributes.with_min_inner_size(LogicalSize::new(
                f64::from(min_size.width.as_f32()),
                f64::from(min_size.height.as_f32()),
            ));
        }
        if !matches!(params.kind, WindowKind::Normal) {
            attributes = attributes.with_window_level(WindowLevel::AlwaysOnTop);
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("creating the winit window")?,
        );
        window.set_enabled_buttons(enabled_buttons(&params));
        let hwnd = hwnd_of(&window)?;

        let inner_size = window.inner_size();
        let renderer = Renderer::new(
            renderer_context,
            window.clone(),
            size(
                DevicePixels(inner_size.width as i32),
                DevicePixels(inner_size.height as i32),
            ),
        )
        .context("creating the wgpu renderer")?;

        let scale_factor = window.scale_factor() as f32;
        let inner = Rc::new(Self {
            renderer: RefCell::new(renderer),
            window,
            handle,
            hwnd,
            cursor,
            capture: RefCell::default(),
            callbacks: Callbacks::default(),
            input_handler: Cell::new(None),
            scale_factor: Cell::new(scale_factor),
            modifiers: Cell::new(Modifiers::default()),
            mouse_position: Cell::new(Point::default()),
            pressed_button: Cell::new(None),
            click_state: RefCell::new(ClickState::default()),
            hovered: Cell::new(false),
            active: Cell::new(false),
            appearance: Cell::new(system::appearance().log_err().unwrap_or_default()),
            background_appearance: Cell::new(WindowBackgroundAppearance::Opaque),
            restore_bounds: Cell::new(Bounds::default()),
            frame_requested: Cell::new(false),
            force_render_pending: Cell::new(false),
        });
        inner.align_client_origin(bounds.origin, scale_factor);
        inner.restore_bounds.set(inner.bounds());
        inner.apply_cursor();
        if params.show {
            inner.window.set_visible(true);
        }
        if params.focus {
            inner.window.focus_window();
        }
        inner.schedule_frame();
        Ok(inner)
    }

    pub(crate) fn id(&self) -> WindowId {
        self.window.id()
    }

    pub(crate) fn handle(&self) -> AnyWindowHandle {
        self.handle
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.get()
    }

    pub(crate) fn is_hovered(&self) -> bool {
        self.hovered.get()
    }

    pub(crate) fn apply_cursor(&self) {
        self.window
            .set_cursor(system::cursor_icon(self.cursor.style.get()));
        self.sync_cursor_visibility();
    }

    pub(crate) fn hide_cursor(&self) {
        self.window.set_cursor_visible(false);
    }

    fn sync_cursor_visibility(&self) {
        self.window
            .set_cursor_visible(self.cursor.visible.get() && !self.is_captured());
    }

    pub(crate) fn is_captured(&self) -> bool {
        self.capture.borrow().is_captured()
    }

    fn capture_cursor(&self) {
        if self.is_captured() {
            return;
        }
        let scale_factor = f64::from(self.scale_factor.get());
        let position = self.mouse_position.get();
        let restore = PhysicalPosition::new(
            f64::from(position.x.as_f32()) * scale_factor,
            f64::from(position.y.as_f32()) * scale_factor,
        );
        let grab = match self.window.set_cursor_grab(CursorGrabMode::Locked) {
            Ok(()) => Grab::Locked,
            Err(_) => match self.window.set_cursor_grab(CursorGrabMode::Confined) {
                Ok(()) => Grab::Confined,
                Err(error) => {
                    log::warn!("unable to capture the cursor: {error}");
                    return;
                }
            },
        };
        self.capture.borrow_mut().begin(grab, restore);
        self.sync_cursor_visibility();
    }

    pub(crate) fn release_capture(&self) {
        let Some(restore) = self.capture.borrow_mut().end() else {
            return;
        };
        self.window
            .set_cursor_grab(CursorGrabMode::None)
            .context("unable to release the cursor")
            .log_err();
        self.window
            .set_cursor_position(restore)
            .context("unable to restore the cursor position")
            .log_err();
        self.sync_cursor_visibility();
    }

    pub(crate) fn handle_raw_motion(&self, (x, y): (f64, f64)) {
        if self.is_captured() {
            self.dispatch(PlatformInput::RawMouseMotion(RawMouseMotionEvent {
                delta: point(x, y),
            }));
        }
    }

    /// Moves the window so its client area starts at `origin`, since winit positions the outer frame.
    fn align_client_origin(&self, origin: Point<Pixels>, scale_factor: f32) {
        let Ok(client) = self.window.inner_position() else {
            return;
        };
        let Ok(outer) = self.window.outer_position() else {
            return;
        };
        let wanted_x = (origin.x.as_f32() * scale_factor) as i32;
        let wanted_y = (origin.y.as_f32() * scale_factor) as i32;
        let target =
            PhysicalPosition::new(outer.x + wanted_x - client.x, outer.y + wanted_y - client.y);
        if target != outer {
            self.window.set_outer_position(target);
        }
    }

    fn logical_size(&self, size: PhysicalSize<u32>) -> Size<Pixels> {
        let scale_factor = self.scale_factor.get();
        gpui::size(
            px(size.width as f32 / scale_factor),
            px(size.height as f32 / scale_factor),
        )
    }

    fn logical_position(&self, position: PhysicalPosition<f64>) -> Point<Pixels> {
        logical_point(
            position.x as f32,
            position.y as f32,
            self.scale_factor.get(),
        )
    }

    fn bounds(&self) -> Bounds<Pixels> {
        let origin = self
            .window
            .inner_position()
            .map(|position| {
                logical_point(
                    position.x as f32,
                    position.y as f32,
                    self.scale_factor.get(),
                )
            })
            .unwrap_or_default();
        Bounds {
            origin,
            size: self.logical_size(self.window.inner_size()),
        }
    }

    fn is_fullscreen(&self) -> bool {
        self.window.fullscreen().is_some()
    }

    fn is_maximized(&self) -> bool {
        !self.is_fullscreen() && self.window.is_maximized()
    }

    fn is_minimized(&self) -> bool {
        self.window.is_minimized().unwrap_or(false)
    }

    fn remember_restore_bounds(&self) {
        let size = self.window.inner_size();
        if size.width > 0
            && size.height > 0
            && !self.is_minimized()
            && !self.is_maximized()
            && !self.is_fullscreen()
        {
            self.restore_bounds.set(self.bounds());
        }
    }

    fn schedule_frame(&self) {
        self.frame_requested.set(true);
        self.window.request_redraw();
    }

    fn paint(&self) {
        let os_initiated = !self.frame_requested.replace(false);
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let force_render = self.force_render_pending.replace(false);
        with_callback(&self.callbacks.request_frame, |request_frame| {
            request_frame(RequestFrameOptions {
                require_presentation: os_initiated,
                force_render,
            });
        });
    }

    fn dispatch(&self, input: PlatformInput) -> Option<DispatchEventResult> {
        with_callback(&self.callbacks.input, |callback| callback(input))
    }

    fn with_input_handler(&self, f: impl FnOnce(&mut PlatformInputHandler)) {
        let Some(mut handler) = self.input_handler.take() else {
            return;
        };
        f(&mut handler);
        // A frame drawn during `f` installs a fresher handler, which wins over this one.
        match self.input_handler.take() {
            Some(fresh) => self.input_handler.set(Some(fresh)),
            None => self.input_handler.set(Some(handler)),
        }
    }

    pub(crate) fn should_close(&self) -> bool {
        with_callback(&self.callbacks.should_close, |callback| callback()).unwrap_or(true)
    }

    pub(crate) fn notify_closed(&self) {
        if let Some(close) = self.callbacks.close.take() {
            close();
        }
    }

    pub(crate) fn handle_event(&self, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => self.handle_resized(size),
            WindowEvent::Moved(_) => {
                self.remember_restore_bounds();
                with_callback(&self.callbacks.moved, |callback| callback());
            }
            WindowEvent::Focused(focused) => self.handle_focused(focused),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor.set(scale_factor as f32);
            }
            WindowEvent::ThemeChanged(theme) => {
                self.appearance.set(match theme {
                    Theme::Light => WindowAppearance::Light,
                    Theme::Dark => WindowAppearance::Dark,
                });
                with_callback(&self.callbacks.appearance_changed, |callback| callback());
            }
            WindowEvent::CursorEntered { .. } => self.set_hovered(true),
            WindowEvent::CursorLeft { .. } => {
                self.set_hovered(false);
                self.dispatch(PlatformInput::MouseExited(MouseExitEvent {
                    position: self.mouse_position.get(),
                    pressed_button: self.pressed_button.get(),
                    modifiers: self.modifiers.get(),
                }));
            }
            WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(position),
            WindowEvent::MouseInput { state, button, .. } => self.handle_mouse_input(state, button),
            WindowEvent::MouseWheel { delta, .. } => self.handle_mouse_wheel(delta),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers.set(input::modifiers(modifiers.state()));
                self.dispatch(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
                    modifiers: self.modifiers.get(),
                    capslock: input::capslock(),
                }));
            }
            WindowEvent::KeyboardInput { event, .. } => self.handle_key(&event),
            WindowEvent::RedrawRequested => self.paint(),
            _ => {}
        }
    }

    fn handle_resized(&self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            self.release_capture();
            return;
        }
        self.renderer.borrow_mut().update_drawable_size(gpui::size(
            DevicePixels(size.width as i32),
            DevicePixels(size.height as i32),
        ));
        self.remember_restore_bounds();
        let logical_size = self.logical_size(size);
        let scale_factor = self.scale_factor.get();
        with_callback(&self.callbacks.resize, |callback| {
            callback(logical_size, scale_factor)
        });
        self.schedule_frame();
    }

    fn handle_focused(&self, focused: bool) {
        self.active.set(focused);
        if !focused {
            self.release_capture();
            self.pressed_button.set(None);
        }
        with_callback(&self.callbacks.active_status_change, |callback| {
            callback(focused)
        });
        self.schedule_frame();
    }

    fn set_hovered(&self, hovered: bool) {
        if self.hovered.replace(hovered) != hovered {
            with_callback(&self.callbacks.hovered_status_change, |callback| {
                callback(hovered)
            });
        }
    }

    fn handle_cursor_moved(&self, position: PhysicalPosition<f64>) {
        if self.is_captured() {
            let recentre = self
                .capture
                .borrow()
                .recentre(position, self.window.inner_size());
            if let Some(center) = recentre {
                self.window
                    .set_cursor_position(center)
                    .context("unable to recentre the cursor")
                    .log_err();
            }
            return;
        }
        if !self.cursor.visible.replace(true) {
            self.window.set_cursor_visible(true);
        }
        let position = self.logical_position(position);
        self.mouse_position.set(position);
        self.dispatch(PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: self.pressed_button.get(),
            modifiers: self.modifiers.get(),
        }));
    }

    fn handle_mouse_input(&self, state: ElementState, button: winit::event::MouseButton) {
        let Some(button) = input::mouse_button(button) else {
            return;
        };
        let position = self.mouse_position.get();
        let modifiers = self.modifiers.get();
        match state {
            ElementState::Pressed => {
                self.pressed_button.set(Some(button));
                let scale_factor = self.scale_factor.get();
                let physical = point(
                    DevicePixels((position.x.as_f32() * scale_factor) as i32),
                    DevicePixels((position.y.as_f32() * scale_factor) as i32),
                );
                let click_count = self.click_state.borrow_mut().update(
                    button,
                    physical,
                    Instant::now(),
                    DoubleClick::system(),
                );
                self.dispatch(PlatformInput::MouseDown(MouseDownEvent {
                    button,
                    position,
                    modifiers,
                    click_count,
                    first_mouse: false,
                }));
            }
            ElementState::Released => {
                if self.pressed_button.get() == Some(button) {
                    self.pressed_button.set(None);
                }
                self.dispatch(PlatformInput::MouseUp(MouseUpEvent {
                    button,
                    position,
                    modifiers,
                    click_count: self.click_state.borrow().current_count(),
                }));
            }
        }
    }

    fn handle_mouse_wheel(&self, delta: MouseScrollDelta) {
        let delta = match delta {
            MouseScrollDelta::LineDelta(x, y) => ScrollDelta::Lines(point(
                x * wheel_setting(SPI_GETWHEELSCROLLCHARS),
                y * wheel_setting(SPI_GETWHEELSCROLLLINES),
            )),
            MouseScrollDelta::PixelDelta(delta) => {
                let scale_factor = self.scale_factor.get() as f64;
                ScrollDelta::Pixels(point(
                    px((delta.x / scale_factor) as f32),
                    px((delta.y / scale_factor) as f32),
                ))
            }
        };
        self.dispatch(PlatformInput::ScrollWheel(ScrollWheelEvent {
            position: self.mouse_position.get(),
            delta,
            modifiers: self.modifiers.get(),
            touch_phase: TouchPhase::Moved,
        }));
    }

    fn handle_key(&self, event: &winit::event::KeyEvent) {
        let Some(translated) = input::translate_key(event, self.modifiers.get()) else {
            return;
        };
        match event.state {
            ElementState::Pressed => {
                let typed = input::types_character(&translated)
                    .then(|| translated.keystroke.key_char.clone())
                    .flatten();
                let handled = self
                    .dispatch(PlatformInput::KeyDown(KeyDownEvent {
                        keystroke: translated.keystroke,
                        is_held: event.repeat,
                        prefer_character_input: translated.prefer_character_input,
                    }))
                    .is_some_and(|result| !result.propagate);
                if !handled && let Some(text) = typed {
                    self.with_input_handler(|handler| {
                        handler.replace_text_in_range(None, &text);
                    });
                }
            }
            ElementState::Released => {
                self.dispatch(PlatformInput::KeyUp(KeyUpEvent {
                    keystroke: translated.keystroke,
                }));
            }
        }
    }
}

/// A GPUI window handed to the application. Dropping it closes the window.
pub(crate) struct WinitWindow {
    inner: Rc<WindowInner>,
    windows: Windows,
}

impl WinitWindow {
    pub(crate) fn new(inner: Rc<WindowInner>, windows: Windows) -> Self {
        Self { inner, windows }
    }
}

impl Drop for WinitWindow {
    fn drop(&mut self) {
        self.inner.release_capture();
        self.windows.borrow_mut().remove(&self.inner.id());
    }
}

impl HasWindowHandle for WinitWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.inner.window.window_handle()
    }
}

impl HasDisplayHandle for WinitWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.inner.window.display_handle()
    }
}

impl PlatformWindow for WinitWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.inner.bounds()
    }

    fn is_maximized(&self) -> bool {
        self.inner.is_maximized()
    }

    fn window_bounds(&self) -> WindowBounds {
        if self.inner.is_fullscreen() {
            WindowBounds::Fullscreen(self.inner.restore_bounds.get())
        } else if self.inner.is_maximized() {
            WindowBounds::Maximized(self.inner.restore_bounds.get())
        } else {
            WindowBounds::Windowed(self.inner.bounds())
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        self.inner.logical_size(self.inner.window.inner_size())
    }

    fn resize(&mut self, size: Size<Pixels>) {
        let size = LogicalSize::new(
            f64::from(size.width.as_f32()),
            f64::from(size.height.as_f32()),
        );
        let _ = self.inner.window.request_inner_size(size);
    }

    fn scale_factor(&self) -> f32 {
        self.inner.scale_factor.get()
    }

    fn appearance(&self) -> WindowAppearance {
        self.inner.appearance.get()
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        WinitDisplay::for_window(self.inner.hwnd).map(|display| Rc::new(display) as _)
    }

    fn mouse_position(&self) -> Point<Pixels> {
        let mut cursor = POINT::default();
        unsafe {
            GetCursorPos(&mut cursor)
                .context("unable to get the cursor position")
                .log_err();
            let _ = ScreenToClient(self.inner.hwnd, &mut cursor);
        }
        logical_point(
            cursor.x as f32,
            cursor.y as f32,
            self.inner.scale_factor.get(),
        )
    }

    fn modifiers(&self) -> Modifiers {
        input::current_modifiers()
    }

    fn capslock(&self) -> Capslock {
        input::capslock()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.inner.input_handler.set(Some(input_handler));
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.inner.input_handler.take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self, token: Option<&str>) -> bool {
        if token.is_some() {
            return false;
        }
        if self.inner.is_minimized() {
            self.inner.window.set_minimized(false);
        }
        self.inner.window.focus_window();
        true
    }

    fn request_attention(&self) {
        if !self.inner.active.get() {
            self.inner
                .window
                .request_user_attention(Some(winit::window::UserAttentionType::Informational));
        }
    }

    fn is_active(&self) -> bool {
        self.inner.active.get()
    }

    fn is_hovered(&self) -> bool {
        self.inner.hovered.get()
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.inner.background_appearance.get()
    }

    fn set_title(&mut self, title: &str) {
        self.inner.window.set_title(title);
    }

    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.inner.background_appearance.set(background_appearance);
        self.inner
            .renderer
            .borrow_mut()
            .update_transparency(!background_appearance.is_opaque());
        self.inner
            .window
            .set_transparent(!background_appearance.is_opaque());
        self.inner
            .window
            .set_system_backdrop(match background_appearance {
                WindowBackgroundAppearance::Opaque | WindowBackgroundAppearance::Transparent => {
                    BackdropType::None
                }
                WindowBackgroundAppearance::Blurred => BackdropType::TransientWindow,
                WindowBackgroundAppearance::MicaBackdrop => BackdropType::MainWindow,
                WindowBackgroundAppearance::MicaAltBackdrop => BackdropType::TabbedWindow,
            });
    }

    fn set_visible(&self, visible: bool) {
        self.inner.window.set_visible(visible);
    }

    fn minimize(&self) {
        self.inner.window.set_minimized(true);
    }

    fn zoom(&self) {
        let maximized = self.inner.is_maximized();
        self.inner.window.set_maximized(!maximized);
    }

    fn toggle_fullscreen(&self) {
        let fullscreen = (!self.inner.is_fullscreen()).then_some(Fullscreen::Borderless(None));
        self.inner.window.set_fullscreen(fullscreen);
    }

    fn is_fullscreen(&self) -> bool {
        self.inner.is_fullscreen()
    }

    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        let inner = Rc::downgrade(&self.inner);
        Some(Rc::new(move || {
            if let Some(inner) = inner.upgrade() {
                inner.schedule_frame();
            }
        }))
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.inner.callbacks.request_frame.set(Some(callback));
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.inner.callbacks.input.set(Some(callback));
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner
            .callbacks
            .active_status_change
            .set(Some(callback));
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner
            .callbacks
            .hovered_status_change
            .set(Some(callback));
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.inner.callbacks.resize.set(Some(callback));
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.inner.callbacks.moved.set(Some(callback));
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.inner.callbacks.should_close.set(Some(callback));
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.inner
            .callbacks
            .hit_test_window_control
            .set(Some(callback));
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.inner.callbacks.close.set(Some(callback));
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.inner.callbacks.appearance_changed.set(Some(callback));
    }

    fn draw(&self, scene: &Scene) {
        if self.inner.renderer.borrow_mut().draw(scene) {
            self.inner.force_render_pending.set(true);
            self.inner.schedule_frame();
        }
    }

    fn schedule_frame(&self) {
        self.inner.schedule_frame();
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.inner.renderer.borrow().sprite_atlas().clone()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        true
    }

    fn get_raw_handle(&self) -> HWND {
        self.inner.hwnd
    }

    fn start_window_move(&self) {
        self.inner.window.drag_window().log_err();
    }

    fn start_window_resize(&self, edge: ResizeEdge) {
        let direction = match edge {
            ResizeEdge::Top => ResizeDirection::North,
            ResizeEdge::TopRight => ResizeDirection::NorthEast,
            ResizeEdge::Right => ResizeDirection::East,
            ResizeEdge::BottomRight => ResizeDirection::SouthEast,
            ResizeEdge::Bottom => ResizeDirection::South,
            ResizeEdge::BottomLeft => ResizeDirection::SouthWest,
            ResizeEdge::Left => ResizeDirection::West,
            ResizeEdge::TopLeft => ResizeDirection::NorthWest,
        };
        self.inner.window.drag_resize_window(direction).log_err();
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        Some(self.inner.renderer.borrow().gpu_specs())
    }

    fn gpu_context(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(self.inner.renderer.borrow().gpu_context()))
    }

    fn gpu_context_info(&self) -> Option<Box<dyn std::any::Any>> {
        self.inner
            .renderer
            .borrow()
            .gpu_context_info()
            .map(|info| Box::new(info) as _)
    }

    fn gpu_device_lost(&self) -> Option<bool> {
        Some(self.inner.renderer.borrow().device_lost())
    }

    fn capture_cursor(&self) {
        self.inner.capture_cursor();
    }

    fn release_cursor(&self) {
        self.inner.release_capture();
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn play_system_bell(&self) {
        let _ = unsafe { MessageBeep(MB_OK) };
    }
}

/// Runs a window callback without holding it, so the callback may re-enter the window.
fn with_callback<F: ?Sized, R>(slot: &Slot<F>, call: impl FnOnce(&mut Box<F>) -> R) -> Option<R> {
    let mut callback = slot.take()?;
    let result = call(&mut callback);
    if slot.take().is_none() {
        slot.set(Some(callback));
    }
    Some(result)
}

fn physical(logical: Pixels, scale_factor: f32) -> u32 {
    (logical.as_f32() * scale_factor).round().max(1.0) as u32
}

fn enabled_buttons(params: &WindowParams) -> winit::window::WindowButtons {
    let mut buttons = winit::window::WindowButtons::CLOSE;
    if params.is_minimizable {
        buttons |= winit::window::WindowButtons::MINIMIZE;
    }
    if params.is_resizable {
        buttons |= winit::window::WindowButtons::MAXIMIZE;
    }
    buttons
}

fn hwnd_of(window: &Window) -> Result<HWND> {
    match window.window_handle()?.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut c_void)),
        other => bail!("unexpected window handle {other:?}"),
    }
}

/// Lines or characters one wheel notch scrolls, as configured in the system settings.
fn wheel_setting(
    action: windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_ACTION,
) -> f32 {
    const DEFAULT: f32 = 3.0;
    let mut value = 0u32;
    let read = unsafe {
        SystemParametersInfoW(
            action,
            0,
            Some(&mut value as *mut u32 as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    // `u32::MAX` asks for a page per notch, which GPUI cannot express in lines.
    if read.is_err() || value == 0 || value == u32::MAX {
        DEFAULT
    } else {
        value as f32
    }
}
