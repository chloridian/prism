//! Native behaviour for custom title bars: the framed window whose caption is removed, hit testing
//! for the caption and its buttons, and the non-client mouse messages that follow from it.
//!
//! A window with a custom title bar keeps winit's normal frame, so Aero Snap, the shadow and the
//! resize borders stay native. `WM_NCCALCSIZE` then gives the caption's space back to the client
//! area, and `WM_NCHITTEST` answers for the caption buttons and the top border. The frame
//! subclasses the window procedure beneath the accessibility adapter, which installs later and
//! unwinds first.

use std::{
    cell::Cell,
    rc::{Rc, Weak},
    sync::Arc,
};

use gpui::{MouseButton, Pixels, Point, WindowControlArea};
use gpui_util::ResultExt as _;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{
        Controls::HOVER_DEFAULT,
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi},
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent,
            VK_LBUTTON, VK_RBUTTON,
        },
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            DefWindowProcW, GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetSystemMetrics,
            GetWindowLongW, GetWindowRect, HTCAPTION, HTCLOSE, HTMAXBUTTON, HTMINBUTTON, HTNOWHERE,
            HTTOP, HTTOPLEFT, HTTOPRIGHT, IsZoomed, MA_ACTIVATE, MINMAXINFO, NCCALCSIZE_PARAMS,
            PostMessageW, SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CYSIZEFRAME, SM_SWAPBUTTON,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOOWNERZORDER,
            SWP_NOREPOSITION, SWP_NOSENDCHANGING, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DPICHANGED, WM_GETMINMAXINFO,
            WM_MOUSEACTIVATE, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
            WM_NCLBUTTONUP, WM_NCMBUTTONDOWN, WM_NCMBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
            WM_NCRBUTTONDOWN, WM_NCRBUTTONUP,
        },
    },
};
use winit::window::WindowId;

use crate::{dispatcher::WinitDispatcher, window::WindowInner};

const SUBCLASS_ID: usize = 1;

/// Non-client mouse events deferred from `subclass_proc` into winit's `user_event` callback
/// so GPUI handlers run with an active event loop.
#[derive(Debug)]
pub(crate) enum NonClientEvent {
    MouseMove {
        window_id: WindowId,
        position: Point<Pixels>,
    },
    MouseLeave {
        window_id: WindowId,
    },
    MouseDown {
        window_id: WindowId,
        button: MouseButton,
        position: Point<Pixels>,
        hit_test: u32,
        screen_lparam: isize,
        double_click: bool,
    },
    MouseUp {
        window_id: WindowId,
        button: MouseButton,
        position: Point<Pixels>,
        hit_test: u32,
        screen_lparam: isize,
    },
}

impl NonClientEvent {
    pub(crate) fn window_id(&self) -> WindowId {
        match *self {
            Self::MouseMove { window_id, .. }
            | Self::MouseLeave { window_id }
            | Self::MouseDown { window_id, .. }
            | Self::MouseUp { window_id, .. } => window_id,
        }
    }
}

/// How a window's frame is drawn and what the user may do with it.
#[derive(Clone, Copy)]
pub(crate) struct FrameStyle {
    /// Whether the application draws its own title bar.
    pub(crate) custom_title_bar: bool,
    pub(crate) movable: bool,
    pub(crate) resizable: bool,
    pub(crate) minimizable: bool,
    pub(crate) has_min_size: bool,
}

struct State {
    window: Weak<WindowInner>,
    dispatcher: Arc<WinitDispatcher>,
    style: FrameStyle,
    pressed_button: Cell<Option<u32>>,
    tracking_leave: Cell<bool>,
}

/// The subclass on a window. Dropping it removes the subclass.
pub(crate) struct Frame {
    hwnd: HWND,
    state: *mut State,
}

impl Frame {
    pub(crate) fn install(
        hwnd: HWND,
        window: Weak<WindowInner>,
        dispatcher: Arc<WinitDispatcher>,
        style: FrameStyle,
    ) -> Self {
        let state = Box::into_raw(Box::new(State {
            window,
            dispatcher,
            style,
            pressed_button: Cell::new(None),
            tracking_leave: Cell::new(false),
        }));
        // SAFETY: `state` stays allocated until `drop` removes the subclass, and the window
        // procedure only runs on this thread.
        let installed =
            unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, state as usize) };
        if !installed.as_bool() {
            log::warn!("unable to subclass the window for its frame");
        }
        if style.custom_title_bar {
            let client = client_size(hwnd);
            notify_frame_changed(hwnd);
            resize_client(hwnd, client);
        }
        Self { hwnd, state }
    }

    pub(crate) fn handle_event(&self, window: &WindowInner, event: NonClientEvent) {
        // SAFETY: `state` stays allocated until `drop`, and this runs on the main thread.
        let state = unsafe { &*self.state };
        state.handle_deferred(window, self.hwnd, event);
    }

    /// Resizes the client area of a window with a custom title bar to `size` in pixels. Returns
    /// whether the frame handled it, because winit sizes the window as if the caption were
    /// still there.
    pub(crate) fn resize_client(&self, size: (i32, i32)) -> bool {
        // SAFETY: `state` stays allocated until `drop`.
        let custom_title_bar = unsafe { &*self.state }.style.custom_title_bar;
        if custom_title_bar {
            resize_client(self.hwnd, size);
        }
        custom_title_bar
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // SAFETY: the subclass is removed before the state it points to is freed.
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(subclass_proc), SUBCLASS_ID);
            drop(Box::from_raw(self.state));
        }
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: `data` is the `State` that `Frame::install` leaked and `Frame::drop` frees only
    // after removing this subclass.
    let state = unsafe { &*(data as *const State) };
    if let Some(window) = state.window.upgrade()
        && let Some(result) = state.handle(&window, hwnd, msg, wparam, lparam)
    {
        return LRESULT(result);
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

impl State {
    fn handle(
        &self,
        window: &Rc<WindowInner>,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<isize> {
        match msg {
            // `DefWindowProc` answers `MA_NOACTIVATE` for a left click on `HTCAPTION` and only
            // activates the window when it handles the following `WM_NCLBUTTONDOWN`. GPUI sees that
            // press first, so a handler that consumes it would leave the window inactive.
            WM_MOUSEACTIVATE => Some(MA_ACTIVATE as isize),
            WM_NCCALCSIZE => self.calc_client_size(window, hwnd, wparam, lparam),
            WM_GETMINMAXINFO => self.get_min_max_info(hwnd, wparam, lparam),
            WM_DPICHANGED => self.dpi_changed(window, hwnd, wparam, lparam),
            WM_NCHITTEST => self.hit_test(window, hwnd, lparam),
            WM_NCMOUSEMOVE => self.mouse_move(window, hwnd, lparam),
            WM_NCMOUSELEAVE => {
                self.tracking_leave.set(false);
                self.dispatcher.send_non_client(NonClientEvent::MouseLeave {
                    window_id: window.id(),
                });
                Some(0)
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => self.mouse_down(
                window,
                hwnd,
                MouseButton::Left,
                wparam,
                lparam,
                msg == WM_NCLBUTTONDBLCLK,
            ),
            WM_NCRBUTTONDOWN => {
                self.mouse_down(window, hwnd, MouseButton::Right, wparam, lparam, false)
            }
            WM_NCMBUTTONDOWN => {
                self.mouse_down(window, hwnd, MouseButton::Middle, wparam, lparam, false)
            }
            WM_NCLBUTTONUP => self.mouse_up(window, hwnd, MouseButton::Left, wparam, lparam),
            WM_NCRBUTTONUP => self.mouse_up(window, hwnd, MouseButton::Right, wparam, lparam),
            WM_NCMBUTTONUP => self.mouse_up(window, hwnd, MouseButton::Middle, wparam, lparam),
            _ => None,
        }
    }

    fn hit_test(&self, window: &WindowInner, hwnd: HWND, lparam: LPARAM) -> Option<isize> {
        if window.is_fullscreen() {
            return None;
        }

        let style = self.style;
        let control = window
            .hit_test_window_control()
            .and_then(|area| match area {
                WindowControlArea::Drag if style.movable => Some(HTCAPTION),
                WindowControlArea::Drag => None,
                WindowControlArea::Close => Some(HTCLOSE),
                WindowControlArea::Max if style.resizable => Some(HTMAXBUTTON),
                WindowControlArea::Max if style.movable => Some(HTCAPTION),
                WindowControlArea::Max => Some(HTNOWHERE),
                WindowControlArea::Min if style.minimizable => Some(HTMINBUTTON),
                WindowControlArea::Min if style.movable => Some(HTCAPTION),
                WindowControlArea::Min => Some(HTNOWHERE),
            });

        if !style.custom_title_bar {
            return control.map(|code| code as isize);
        }

        // The caption is gone, so `DefWindowProc` only registers a one pixel border at the top
        // edge. The frame thickness is measured here and the hit test done by hand.
        // SAFETY: `hwnd` is the live window this message was sent to.
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let frame_y = frame_thickness_x(dpi);
        let frame_x = frame_thickness_y(dpi);
        let cursor = client_point(hwnd, lparam);
        if style.resizable && !is_maximized(window, hwnd) && 0 <= cursor.y && cursor.y <= frame_y {
            // The x axis runs from `-frame_x` to 0 across the left border.
            let border = if cursor.x <= 0 {
                HTTOPLEFT
            } else {
                let mut rect = RECT::default();
                // SAFETY: as above.
                unsafe { GetWindowRect(hwnd, &mut rect) }.log_err();
                // The right and bottom bounds of a `RECT` are exclusive.
                let right = rect.right - rect.left - 1;
                // The bounds include the padding frames, so allow for both of them.
                if right - 2 * frame_x <= cursor.x {
                    HTTOPRIGHT
                } else {
                    HTTOP
                }
            };
            return Some(border as isize);
        }
        control.map(|code| code as isize)
    }

    fn calc_client_size(
        &self,
        window: &WindowInner,
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<isize> {
        if !self.style.custom_title_bar || window.is_fullscreen() || wparam.0 == 0 {
            return None;
        }

        // SAFETY: with `wparam` set, `lparam` points to the `NCCALCSIZE_PARAMS` of this message.
        unsafe {
            let params = lparam.0 as *mut NCCALCSIZE_PARAMS;
            let saved_top = (*params).rgrc[0].top;
            let result = DefWindowProcW(hwnd, WM_NCCALCSIZE, wparam, lparam);
            (*params).rgrc[0].top = saved_top;
            if is_maximized(window, hwnd) {
                (*params).rgrc[0].top += frame_thickness_x(GetDpiForWindow(hwnd));
            }
            Some(result.0)
        }
    }

    /// winit derives the track size from the outer size of a window with a caption, which this
    /// window no longer has.
    fn get_min_max_info(&self, hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<isize> {
        if !self.style.custom_title_bar || !self.style.has_min_size {
            return None;
        }
        // SAFETY: `lparam` points to the `MINMAXINFO` of this message.
        unsafe {
            let result = DefSubclassProc(hwnd, WM_GETMINMAXINFO, wparam, lparam);
            (*(lparam.0 as *mut MINMAXINFO)).ptMinTrackSize.y -= caption_extent(hwnd);
            Some(result.0)
        }
    }

    /// winit resizes the window for the new scale factor as if the caption were still there.
    fn dpi_changed(
        &self,
        window: &WindowInner,
        hwnd: HWND,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<isize> {
        if !self.style.custom_title_bar {
            return None;
        }
        // SAFETY: the message is forwarded unchanged.
        let result = unsafe { DefSubclassProc(hwnd, WM_DPICHANGED, wparam, lparam) };
        if !is_maximized(window, hwnd) && !window.is_fullscreen() {
            let mut rect = RECT::default();
            // SAFETY: `hwnd` is the live window this message was sent to.
            if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
                set_outer_size(
                    hwnd,
                    (
                        rect.right - rect.left,
                        rect.bottom - rect.top - caption_extent(hwnd),
                    ),
                );
            }
        }
        Some(result.0)
    }

    fn mouse_move(&self, window: &WindowInner, hwnd: HWND, lparam: LPARAM) -> Option<isize> {
        if !self.tracking_leave.replace(true) {
            // SAFETY: `hwnd` is the live window this message was sent to.
            unsafe {
                TrackMouseEvent(&mut TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: HOVER_DEFAULT,
                })
                .log_err();
            }
        }
        let position = window.logical_client_position(client_point(hwnd, lparam));
        self.dispatcher.send_non_client(NonClientEvent::MouseMove {
            window_id: window.id(),
            position,
        });
        None
    }

    fn mouse_down(
        &self,
        window: &WindowInner,
        hwnd: HWND,
        button: MouseButton,
        wparam: WPARAM,
        lparam: LPARAM,
        double_click: bool,
    ) -> Option<isize> {
        let hit_test = wparam.0 as u32;
        if !is_caption_hit_test(hit_test) {
            return None;
        }
        let position = window.logical_client_position(client_point(hwnd, lparam));
        self.dispatcher.send_non_client(NonClientEvent::MouseDown {
            window_id: window.id(),
            button,
            position,
            hit_test,
            screen_lparam: lparam.0,
            double_click,
        });
        Some(0)
    }

    fn mouse_up(
        &self,
        window: &WindowInner,
        hwnd: HWND,
        button: MouseButton,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<isize> {
        let hit_test = wparam.0 as u32;
        if !is_caption_hit_test(hit_test) {
            return None;
        }
        let position = window.logical_client_position(client_point(hwnd, lparam));
        self.dispatcher.send_non_client(NonClientEvent::MouseUp {
            window_id: window.id(),
            button,
            position,
            hit_test,
            screen_lparam: lparam.0,
        });
        Some(0)
    }

    fn handle_deferred(&self, window: &WindowInner, hwnd: HWND, event: NonClientEvent) {
        match event {
            NonClientEvent::MouseMove { position, .. } => {
                window.nc_mouse_move(position);
            }
            NonClientEvent::MouseLeave { .. } => {
                window.cursor_left();
            }
            NonClientEvent::MouseDown {
                button,
                position,
                hit_test,
                screen_lparam,
                double_click,
                ..
            } => {
                if window.nc_mouse_down(button, position) {
                    return;
                }
                if button != MouseButton::Left {
                    return;
                }
                match hit_test {
                    code @ (HTMINBUTTON | HTMAXBUTTON | HTCLOSE) => {
                        self.pressed_button.set(Some(code));
                    }
                    HTCAPTION if double_click => {
                        // SAFETY: `hwnd` is live for the duration of `window`.
                        unsafe {
                            DefWindowProcW(
                                hwnd,
                                WM_NCLBUTTONDBLCLK,
                                WPARAM(HTCAPTION as usize),
                                LPARAM(screen_lparam),
                            );
                        }
                    }
                    HTCAPTION if primary_mouse_button_down() => {
                        // SAFETY: `hwnd` is live for the duration of `window`.
                        unsafe {
                            DefWindowProcW(
                                hwnd,
                                WM_NCLBUTTONDOWN,
                                WPARAM(HTCAPTION as usize),
                                LPARAM(screen_lparam),
                            );
                        }
                    }
                    _ => {}
                }
            }
            NonClientEvent::MouseUp {
                button,
                position,
                hit_test,
                screen_lparam,
                ..
            } => {
                if window.nc_mouse_up(button, position) {
                    return;
                }
                let pressed = self.pressed_button.take();
                match (button, hit_test, pressed) {
                    (MouseButton::Left, HTMINBUTTON, Some(HTMINBUTTON))
                        if self.style.minimizable =>
                    {
                        window.minimize();
                    }
                    (MouseButton::Left, HTMAXBUTTON, Some(HTMAXBUTTON)) if self.style.resizable => {
                        window.toggle_maximized();
                    }
                    (MouseButton::Left, HTCLOSE, Some(HTCLOSE)) => {
                        // SAFETY: `hwnd` is live for the duration of `window`.
                        unsafe {
                            PostMessageW(Some(hwnd), WM_CLOSE, WPARAM::default(), LPARAM::default())
                        }
                        .log_err();
                    }
                    (MouseButton::Right, HTCAPTION, _) => {
                        // SAFETY: `hwnd` is live for the duration of `window`.
                        unsafe {
                            DefWindowProcW(
                                hwnd,
                                WM_NCRBUTTONUP,
                                WPARAM(HTCAPTION as usize),
                                LPARAM(screen_lparam),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn is_caption_hit_test(code: u32) -> bool {
    matches!(code, HTCAPTION | HTMINBUTTON | HTMAXBUTTON | HTCLOSE)
}

fn primary_mouse_button_down() -> bool {
    // SAFETY: plain system metric and key state queries.
    unsafe {
        let vk = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        GetAsyncKeyState(i32::from(vk.0)) < 0
    }
}

/// Converts the screen position packed in the `lparam` of a non-client mouse message.
fn client_point(hwnd: HWND, lparam: LPARAM) -> POINT {
    let mut point = POINT {
        x: i32::from(lparam.0 as i16),
        y: i32::from((lparam.0 >> 16) as i16),
    };
    // SAFETY: `hwnd` is the live window this message was sent to.
    unsafe { ScreenToClient(hwnd, &mut point) }.ok().log_err();
    point
}

/// winit only learns that the window is maximized from `WM_SIZE`, which comes after the
/// `WM_NCCALCSIZE` that depends on it, so the window style is the source of truth here.
fn is_maximized(window: &WindowInner, hwnd: HWND) -> bool {
    // SAFETY: `hwnd` is a live window.
    !window.is_fullscreen() && unsafe { IsZoomed(hwnd) }.as_bool()
}

fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut client = RECT::default();
    // SAFETY: `hwnd` is a live window.
    unsafe { GetClientRect(hwnd, &mut client) }.log_err();
    (client.right - client.left, client.bottom - client.top)
}

fn set_outer_size(hwnd: HWND, (width, height): (i32, i32)) {
    // SAFETY: `hwnd` is a live window.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .log_err();
}

/// Sizes the window so its client area is `client`, keeping the frame it has now.
fn resize_client(hwnd: HWND, client: (i32, i32)) {
    let mut rect = RECT::default();
    // SAFETY: `hwnd` is a live window.
    if unsafe { GetWindowRect(hwnd, &mut rect) }
        .log_err()
        .is_none()
    {
        return;
    }
    let current = client_size(hwnd);
    set_outer_size(
        hwnd,
        (
            rect.right - rect.left - current.0 + client.0,
            rect.bottom - rect.top - current.1 + client.1,
        ),
    );
}

/// The height of the caption and top border that a framed window of this style has and this
/// window gives back to its client area.
fn caption_extent(hwnd: HWND) -> i32 {
    let mut rect = RECT::default();
    // SAFETY: `hwnd` is a live window.
    unsafe {
        AdjustWindowRectExForDpi(
            &mut rect,
            WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32),
            false,
            WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32),
            GetDpiForWindow(hwnd),
        )
    }
    .log_err();
    -rect.top
}

// There is some additional non-visible space around a window's border:
// - `SM_CXSIZEFRAME`: the resize handle.
// - `SM_CXPADDEDBORDER`: additional border space that isn't part of the resize handle.
fn frame_thickness_x(dpi: u32) -> i32 {
    // SAFETY: plain system metric queries.
    unsafe {
        GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

fn frame_thickness_y(dpi: u32) -> i32 {
    // SAFETY: as above.
    unsafe {
        GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
    }
}

fn notify_frame_changed(hwnd: HWND) {
    // SAFETY: `hwnd` is a live window.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED
                | SWP_NOACTIVATE
                | SWP_NOCOPYBITS
                | SWP_NOMOVE
                | SWP_NOOWNERZORDER
                | SWP_NOREPOSITION
                | SWP_NOSENDCHANGING
                | SWP_NOSIZE
                | SWP_NOZORDER,
        )
    }
    .log_err();
}
