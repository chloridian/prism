//! The winit application handler: routes loop events to the platform's tasks and windows.

use std::rc::Rc;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

use crate::{dispatcher::LoopEvent, platform::Shared};

pub(crate) struct App {
    shared: Rc<Shared>,
    on_finish_launching: Option<Box<dyn FnOnce()>>,
}

impl App {
    pub(crate) fn new(shared: Rc<Shared>, on_finish_launching: Box<dyn FnOnce()>) -> Self {
        Self {
            shared,
            on_finish_launching: Some(on_finish_launching),
        }
    }
}

impl ApplicationHandler<LoopEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let _scope = self.shared.enter(event_loop);
        if let Some(on_finish_launching) = self.on_finish_launching.take() {
            event_loop.set_control_flow(ControlFlow::Wait);
            on_finish_launching();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: LoopEvent) {
        let _scope = self.shared.enter(event_loop);
        match event {
            LoopEvent::Wake => self.shared.run_main_tasks(),
            LoopEvent::NonClient(event) => {
                if let Some(window) = self
                    .shared
                    .windows
                    .borrow()
                    .get(&event.window_id())
                    .cloned()
                {
                    window.handle_non_client(event);
                }
            }
        }
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let DeviceEvent::MouseMotion { delta } = event else {
            return;
        };
        let _scope = self.shared.enter(event_loop);
        let captured = self
            .shared
            .windows
            .borrow()
            .values()
            .find(|window| window.is_captured())
            .cloned();
        if let Some(window) = captured {
            window.handle_raw_motion(delta);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let _scope = self.shared.enter(event_loop);
        let Some(window) = self.shared.windows.borrow().get(&window_id).cloned() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                window.release_capture();
                if window.should_close() {
                    self.shared.windows.borrow_mut().remove(&window_id);
                    window.notify_closed();
                }
            }
            WindowEvent::Focused(true) => {
                window.handle_event(event);
                self.shared.check_keyboard_layout();
            }
            event => window.handle_event(event),
        }
    }
}
