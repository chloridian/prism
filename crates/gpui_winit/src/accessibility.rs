//! Exposes GPUI's accessibility tree to UI Automation through AccessKit.

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_windows::SubclassingAdapter;
use gpui::A11yCallbacks;
use windows::Win32::Foundation::HWND;

/// The adapter that answers `WM_GETOBJECT` for a window.
///
/// winit owns the window procedure, so the adapter subclasses it. That requires installing it
/// before the window is first shown.
pub(crate) struct Accessibility {
    adapter: SubclassingAdapter,
}

impl Accessibility {
    pub(crate) fn new(hwnd: HWND, callbacks: A11yCallbacks) -> Self {
        Self {
            adapter: SubclassingAdapter::new(
                hwnd,
                Activation(callbacks.activation),
                Action(callbacks.action),
            ),
        }
    }

    pub(crate) fn update(&mut self, tree_update: TreeUpdate) {
        // The events must be raised after the adapter is released: raising them can send a nested
        // `WM_GETOBJECT` to the window.
        if let Some(events) = self.adapter.update_if_active(|| tree_update) {
            events.raise();
        }
    }
}

struct Activation(Box<dyn Fn() -> Option<TreeUpdate> + Send + 'static>);

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        (self.0)()
    }
}

struct Action(Box<dyn Fn(ActionRequest) + Send + 'static>);

impl ActionHandler for Action {
    fn do_action(&mut self, request: ActionRequest) {
        (self.0)(request);
    }
}
