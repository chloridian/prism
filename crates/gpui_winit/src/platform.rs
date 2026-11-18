//! The winit `Platform`: process-wide services and the state its windows share.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
    sync::Arc,
    time::Instant,
};

use anyhow::{Context as _, Result, anyhow, bail};
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, BackgroundExecutor, ClipboardItem, CursorStyle, ForegroundExecutor,
    Keymap, Menu, MenuItem, PathPromptOptions, Platform, PlatformDisplay, PlatformKeyboardLayout,
    PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, PriorityQueueReceiver,
    RunnableVariant, Task, ThermalState, WindowAppearance, WindowParams,
};
use gpui_util::ResultExt as _;
use windows::Win32::System::Ole::OleInitialize;
use winit::event_loop::{ActiveEventLoop, EventLoop};

use crate::{
    app::App,
    clipboard, credentials,
    dispatcher::{MAIN_TASK_BUDGET, Wake, WinitDispatcher, execute_runnable},
    display::WinitDisplay,
    keyboard::{WindowsKeyboardLayout, WindowsKeyboardMapper},
    renderer::RendererContext,
    system,
    text::DirectWriteTextSystem,
    window::{CursorState, WindowInner, Windows, WinitWindow},
};

/// A GPUI platform whose windows, input and event loop come from winit.
pub struct WinitPlatform {
    shared: Rc<Shared>,
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    text_system: Arc<DirectWriteTextSystem>,
}

/// The state the platform shares with the event loop handler.
pub(crate) struct Shared {
    dispatcher: Arc<WinitDispatcher>,
    main_receiver: PriorityQueueReceiver<RunnableVariant>,
    event_loop: Cell<*const ActiveEventLoop>,
    pub(crate) windows: Windows,
    cursor: Rc<CursorState>,
    renderer_context: RefCell<RendererContext>,
    quit: Cell<Option<Box<dyn FnMut() -> bool>>>,
    keyboard_layout_change: Cell<Option<Box<dyn FnMut()>>>,
    keyboard_layout_id: RefCell<String>,
}

/// Keeps the active event loop reachable for the duration of one winit callback.
pub(crate) struct EventLoopScope<'a> {
    shared: &'a Shared,
}

impl Drop for EventLoopScope<'_> {
    fn drop(&mut self) {
        self.shared.event_loop.set(ptr::null());
    }
}

impl Shared {
    pub(crate) fn enter<'a>(&'a self, event_loop: &ActiveEventLoop) -> EventLoopScope<'a> {
        self.event_loop.set(event_loop);
        EventLoopScope { shared: self }
    }

    /// Runs `f` with the event loop of the winit callback in progress, if any.
    fn with_event_loop<R>(&self, f: impl FnOnce(&ActiveEventLoop) -> R) -> Option<R> {
        let event_loop = self.event_loop.get();
        // SAFETY: the pointer is set by `enter` from a live `&ActiveEventLoop` and cleared when the
        // returned scope drops at the end of that callback. Everything here runs on the main thread.
        unsafe { event_loop.as_ref() }.map(f)
    }

    pub(crate) fn exit(&self) {
        if self.with_event_loop(ActiveEventLoop::exit).is_none() {
            log::warn!("quit requested outside the event loop");
        }
    }

    pub(crate) fn run_main_tasks(&self) {
        let start = Instant::now();
        loop {
            loop {
                if start.elapsed() >= MAIN_TASK_BUDGET {
                    // Yield so pending window events run before the remaining tasks.
                    self.dispatcher.rewake();
                    return;
                }
                match self.pop_main_task() {
                    Some(runnable) => execute_runnable(runnable),
                    None => break,
                }
            }

            // A task queued after the last pop saw a set flag and did not wake the loop.
            self.dispatcher.clear_wake();
            match self.pop_main_task() {
                Some(runnable) => {
                    self.dispatcher.mark_woken();
                    execute_runnable(runnable);
                }
                None => return,
            }
        }
    }

    fn pop_main_task(&self) -> Option<RunnableVariant> {
        self.main_receiver.clone().try_pop().ok().flatten()
    }

    pub(crate) fn run_quit_callback(&self) {
        if let Some(mut quit) = self.quit.take() {
            quit();
            if self.quit.take().is_none() {
                self.quit.set(Some(quit));
            }
        }
    }

    pub(crate) fn check_keyboard_layout(&self) {
        let Some(layout) = WindowsKeyboardLayout::new().log_err() else {
            return;
        };
        let id = layout.id().to_owned();
        if self.keyboard_layout_id.replace(id.clone()) == id {
            return;
        }
        if let Some(mut callback) = self.keyboard_layout_change.take() {
            callback();
            if self.keyboard_layout_change.take().is_none() {
                self.keyboard_layout_change.set(Some(callback));
            }
        }
    }

    fn windows_snapshot(&self) -> Vec<Rc<WindowInner>> {
        self.windows.borrow().values().cloned().collect()
    }
}

impl WinitPlatform {
    pub fn new(headless: bool) -> Result<Self> {
        let _ = headless;
        unsafe { OleInitialize(None) }.context("unable to initialize Windows OLE")?;
        let text_system = Arc::new(
            DirectWriteTextSystem::new().context("error creating the DirectWrite text system")?,
        );
        let (dispatcher, main_receiver) = WinitDispatcher::new();
        let dispatcher = Arc::new(dispatcher);
        let keyboard_layout_id = WindowsKeyboardLayout::new()
            .log_err()
            .map(|layout| layout.id().to_owned())
            .unwrap_or_default();

        Ok(Self {
            shared: Rc::new(Shared {
                dispatcher: dispatcher.clone(),
                main_receiver,
                event_loop: Cell::new(ptr::null()),
                windows: Rc::new(RefCell::new(HashMap::new())),
                cursor: Rc::new(CursorState::default()),
                renderer_context: RefCell::new(RendererContext::default()),
                quit: Cell::new(None),
                keyboard_layout_change: Cell::new(None),
                keyboard_layout_id: RefCell::new(keyboard_layout_id),
            }),
            background_executor: BackgroundExecutor::new(dispatcher.clone()),
            foreground_executor: ForegroundExecutor::new(dispatcher),
            text_system,
        })
    }
}

impl Platform for WinitPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.background_executor.clone()
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.foreground_executor.clone()
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.text_system.clone()
    }

    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        system::initialize_com();
        let event_loop = match EventLoop::<Wake>::with_user_event().build() {
            Ok(event_loop) => event_loop,
            Err(error) => {
                log::error!("failed to create the event loop: {error}");
                return;
            }
        };
        self.shared.dispatcher.attach(event_loop.create_proxy());

        let mut app = App::new(self.shared.clone(), on_finish_launching);
        if let Err(error) = event_loop.run_app(&mut app) {
            log::error!("the event loop failed: {error}");
        }
        self.shared.run_quit_callback();
    }

    fn quit(&self) {
        let shared = self.shared.clone();
        self.foreground_executor
            .spawn(async move { shared.exit() })
            .detach();
    }

    fn restart(&self, binary_path: Option<PathBuf>, arguments: Vec<OsString>) {
        let Some(app_path) = binary_path.or_else(|| self.app_path().log_err()) else {
            return;
        };
        let shared = self.shared.clone();
        // Spawning after the current callback unwinds keeps the app borrow released.
        self.foreground_executor
            .spawn(async move {
                #[allow(
                    clippy::disallowed_methods,
                    reason = "the app restarts itself, so a blocking spawn is fine"
                )]
                match std::process::Command::new(app_path).args(arguments).spawn() {
                    Ok(_) => shared.exit(),
                    Err(error) => log::error!("failed to restart: {error}"),
                }
            })
            .detach();
    }

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        WinitDisplay::all()
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        WinitDisplay::primary().map(|display| Rc::new(display) as _)
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.shared
            .windows_snapshot()
            .into_iter()
            .find(|window| window.is_active())
            .map(|window| window.handle())
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        options: WindowParams,
    ) -> Result<Box<dyn PlatformWindow>> {
        let renderer_context = self.shared.renderer_context.borrow().clone();
        let cursor = self.shared.cursor.clone();
        let inner = self
            .shared
            .with_event_loop(|event_loop| {
                WindowInner::open(event_loop, handle, options, renderer_context, cursor)
            })
            .context("windows can only be opened while the event loop is running")??;
        self.shared
            .windows
            .borrow_mut()
            .insert(inner.id(), inner.clone());
        Ok(Box::new(WinitWindow::new(
            inner,
            self.shared.windows.clone(),
        )))
    }

    fn window_appearance(&self) -> WindowAppearance {
        system::appearance().log_err().unwrap_or_default()
    }

    fn open_url(&self, url: &str) {
        if url.is_empty() {
            return;
        }
        let url = url.to_owned();
        self.background_executor
            .spawn(async move {
                system::open(&url)
                    .with_context(|| format!("opening {url}"))
                    .log_err();
            })
            .detach();
    }

    fn on_open_urls(&self, _callback: Box<dyn FnMut(Vec<String>)>) {}

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(anyhow!("URL schemes are not supported")))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        tx.send(Err(anyhow!("file dialogs are not supported"))).ok();
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        tx.send(Err(anyhow!("file dialogs are not supported"))).ok();
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, path: &Path) {
        if path.as_os_str().is_empty() {
            return;
        }
        let path = path.to_path_buf();
        self.background_executor
            .spawn(async move {
                system::reveal(&path)
                    .with_context(|| format!("revealing {}", path.display()))
                    .log_err();
            })
            .detach();
    }

    fn open_with_system(&self, path: &Path) {
        if path.as_os_str().is_empty() {
            return;
        }
        let path = path.to_path_buf();
        self.background_executor
            .spawn(async move {
                system::open(&path)
                    .with_context(|| format!("opening {}", path.display()))
                    .log_err();
            })
            .detach();
    }

    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.shared.quit.set(Some(callback));
    }

    fn on_reopen(&self, _callback: Box<dyn FnMut()>) {}

    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {}

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {}

    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {}

    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {}

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

    fn app_path(&self) -> Result<PathBuf> {
        Ok(std::env::current_exe()?)
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        bail!("auxiliary executables are not supported")
    }

    fn set_cursor_style(&self, style: CursorStyle) {
        if self.shared.cursor.style.replace(style) == style {
            return;
        }
        for window in self.shared.windows_snapshot() {
            window.apply_cursor();
        }
    }

    fn hide_cursor_until_mouse_moves(&self) {
        if !self.shared.cursor.visible.replace(false) {
            return;
        }
        if let Some(window) = self
            .shared
            .windows_snapshot()
            .into_iter()
            .find(|window| window.is_hovered())
        {
            window.hide_cursor();
        }
    }

    fn is_cursor_visible(&self) -> bool {
        self.shared.cursor.visible.get()
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        system::should_auto_hide_scrollbars()
            .log_err()
            .unwrap_or(false)
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        clipboard::read()
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        clipboard::write(&item);
    }

    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> {
        let (url, username, password) = (url.to_owned(), username.to_owned(), password.to_vec());
        self.foreground_executor
            .spawn(async move { credentials::write(&url, &username, &password) })
    }

    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        let url = url.to_owned();
        self.foreground_executor
            .spawn(async move { credentials::read(&url) })
    }

    fn delete_credentials(&self, url: &str) -> Task<Result<()>> {
        let url = url.to_owned();
        self.foreground_executor
            .spawn(async move { credentials::delete(&url) })
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(
            WindowsKeyboardLayout::new()
                .log_err()
                .unwrap_or_else(WindowsKeyboardLayout::unknown),
        )
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(WindowsKeyboardMapper::new())
    }

    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {
        self.shared.keyboard_layout_change.set(Some(callback));
    }

    fn set_gpu_requirements(&self, requirements: Box<dyn std::any::Any>) {
        match requirements.downcast::<gpui_wgpu::WgpuDeviceRequirements>() {
            Ok(requirements) => self
                .shared
                .renderer_context
                .borrow_mut()
                .set_requirements(*requirements),
            Err(_) => {
                log::warn!("set_gpu_requirements: expected WgpuDeviceRequirements");
            }
        }
    }
}
