//! The winit application handler: routes loop events to the platform's tasks and windows.

use std::rc::Rc;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

use crate::{dispatcher::Wake, platform::Shared};

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

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let _scope = self.shared.enter(event_loop);
        if let Some(on_finish_launching) = self.on_finish_launching.take() {
            event_loop.set_control_flow(ControlFlow::Wait);
            on_finish_launching();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, _event: Wake) {
        let _scope = self.shared.enter(event_loop);
        self.shared.run_main_tasks();
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
