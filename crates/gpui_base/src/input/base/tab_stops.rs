//! Tab stops for shaped lines.
//!
//! Prism's text system gives a tab the width its shaper chooses and has no tab stops, so the
//! editor measures with [`next_tab_stop`] only where the shaper agrees.

/// The tab stop that a tab starting at `x` ends on, for stops every `stop` pixels.
///
/// Text that fills its columns ends exactly on a stop, and the tab after it takes a whole
/// stop. Glyph positions are `f32` and can fall a hair short of that stop, which would give
/// the tab no width. So a start within `TAB_STOP_SNAP` of the next stop counts as on it. The
/// wrapper measures with the same rule, so both agree on where a tab ends.
pub(crate) fn next_tab_stop(x: f64, stop: f64) -> f64 {
    let snap = (x * TAB_STOP_SNAP_RATIO).clamp(TAB_STOP_SNAP_MIN, TAB_STOP_SNAP_MAX);
    stop * (((x + snap) / stop).floor() + 1.)
}

/// Share of a tab's start position that it may fall short of a stop and still count as on it.
/// Positions summed in `f32` drift by about half of this.
const TAB_STOP_SNAP_RATIO: f64 = 1e-4;
/// Smallest and largest snap distance, in pixels. The largest stays well under the width of a
/// character, so text that ends one column before a stop is never snapped onto it.
const TAB_STOP_SNAP_MIN: f64 = 0.01;
const TAB_STOP_SNAP_MAX: f64 = 0.5;
