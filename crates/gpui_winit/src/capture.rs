//! Cursor capture state. The transitions are pure; the window applies them to the winit window.

use winit::dpi::{PhysicalPosition, PhysicalSize};

/// How the operating system holds the cursor while it is captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grab {
    /// The cursor stays where it is.
    Locked,
    /// The cursor may move inside the window, so it is recentred before it reaches an edge.
    Confined,
}

#[derive(Default)]
pub(crate) struct Capture {
    active: Option<Active>,
}

struct Active {
    grab: Grab,
    restore: PhysicalPosition<f64>,
}

impl Capture {
    pub(crate) fn is_captured(&self) -> bool {
        self.active.is_some()
    }

    /// Starts a capture that puts the cursor back at `restore` when it ends.
    /// Returns false, changing nothing, when one is already active.
    pub(crate) fn begin(&mut self, grab: Grab, restore: PhysicalPosition<f64>) -> bool {
        if self.active.is_some() {
            return false;
        }
        self.active = Some(Active { grab, restore });
        true
    }

    /// Ends the capture and returns where the cursor goes back to.
    pub(crate) fn end(&mut self) -> Option<PhysicalPosition<f64>> {
        self.active.take().map(|active| active.restore)
    }

    /// Where a confined cursor must be recentred after moving to `position`.
    /// The cursor is left alone while it stays in the middle half of the window.
    pub(crate) fn recentre(
        &self,
        position: PhysicalPosition<f64>,
        window: PhysicalSize<u32>,
    ) -> Option<PhysicalPosition<f64>> {
        if self.active.as_ref()?.grab != Grab::Confined {
            return None;
        }
        let center = PhysicalPosition::new(
            f64::from(window.width) / 2.0,
            f64::from(window.height) / 2.0,
        );
        let near_center = (position.x - center.x).abs() <= f64::from(window.width) / 4.0
            && (position.y - center.y).abs() <= f64::from(window.height) / 4.0;
        (!near_center).then_some(center)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: PhysicalSize<u32> = PhysicalSize::new(800, 600);

    fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x, y)
    }

    #[test]
    fn ending_returns_the_position_the_capture_began_at() {
        let mut capture = Capture::default();
        assert!(capture.begin(Grab::Locked, at(10.0, 20.0)));
        assert!(capture.is_captured());
        assert_eq!(capture.end(), Some(at(10.0, 20.0)));
        assert!(!capture.is_captured());
    }

    #[test]
    fn ending_twice_restores_only_once() {
        let mut capture = Capture::default();
        capture.begin(Grab::Locked, at(1.0, 2.0));
        assert!(capture.end().is_some());
        assert_eq!(capture.end(), None);
    }

    #[test]
    fn a_second_begin_keeps_the_first_restore_position() {
        let mut capture = Capture::default();
        assert!(capture.begin(Grab::Locked, at(1.0, 2.0)));
        assert!(!capture.begin(Grab::Confined, at(30.0, 40.0)));
        assert_eq!(capture.end(), Some(at(1.0, 2.0)));
    }

    #[test]
    fn a_locked_cursor_is_never_recentred() {
        let mut capture = Capture::default();
        capture.begin(Grab::Locked, at(0.0, 0.0));
        assert_eq!(capture.recentre(at(799.0, 599.0), WINDOW), None);
    }

    #[test]
    fn a_confined_cursor_is_recentred_once_it_leaves_the_middle() {
        let mut capture = Capture::default();
        capture.begin(Grab::Confined, at(0.0, 0.0));
        assert_eq!(capture.recentre(at(450.0, 350.0), WINDOW), None);
        assert_eq!(
            capture.recentre(at(700.0, 300.0), WINDOW),
            Some(at(400.0, 300.0))
        );
        assert_eq!(
            capture.recentre(at(400.0, 20.0), WINDOW),
            Some(at(400.0, 300.0))
        );
    }

    #[test]
    fn nothing_is_recentred_without_a_capture() {
        assert_eq!(Capture::default().recentre(at(799.0, 599.0), WINDOW), None);
    }
}
