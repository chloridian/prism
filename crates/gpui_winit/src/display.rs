//! Monitors, enumerated through Win32 so they are available outside winit callbacks.

use std::rc::Rc;

use anyhow::{Result, bail};
use gpui::{Bounds, DisplayId, Pixels, PlatformDisplay, Point, point, px, size};
use gpui_util::ResultExt as _;
use itertools::Itertools as _;
use uuid::Uuid;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
            MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
            MonitorFromWindow,
        },
        UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        UI::WindowsAndMessaging::USER_DEFAULT_SCREEN_DPI,
    },
    core::BOOL,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct WinitDisplay {
    handle: HMONITOR,
    scale_factor: f32,
    bounds: Bounds<Pixels>,
    visible_bounds: Bounds<Pixels>,
    uuid: Uuid,
}

// An `HMONITOR` is a process-wide handle that any thread may use.
unsafe impl Send for WinitDisplay {}
unsafe impl Sync for WinitDisplay {}

impl WinitDisplay {
    pub(crate) fn new(display_id: DisplayId) -> Option<Self> {
        Self::from_monitor(monitor_from_display_id(display_id))
    }

    fn from_monitor(handle: HMONITOR) -> Option<Self> {
        let info = monitor_info(handle).log_err()?;
        let monitor = info.monitorInfo.rcMonitor;
        let work = info.monitorInfo.rcWork;
        let scale_factor = monitor_scale_factor(handle).log_err()?;
        Some(Self {
            handle,
            scale_factor,
            bounds: logical_bounds(monitor, scale_factor),
            visible_bounds: logical_bounds(work, scale_factor),
            uuid: uuid_for_device_name(&info.szDevice),
        })
    }

    pub(crate) fn all() -> Vec<Rc<dyn PlatformDisplay>> {
        enumerate_monitors()
            .into_iter()
            .filter_map(|handle| {
                Some(Rc::new(Self::from_monitor(handle)?) as Rc<dyn PlatformDisplay>)
            })
            .collect()
    }

    pub(crate) fn primary() -> Option<Self> {
        // https://devblogs.microsoft.com/oldnewthing/20070809-00/?p=25643
        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        if monitor.is_invalid() {
            log::error!("cannot find the primary monitor");
            return None;
        }
        Self::from_monitor(monitor)
    }

    pub(crate) fn for_window(hwnd: HWND) -> Option<Self> {
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_invalid() {
            return None;
        }
        Self::from_monitor(monitor)
    }

    pub(crate) fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
}

impl PlatformDisplay for WinitDisplay {
    fn id(&self) -> DisplayId {
        display_id_for_monitor(self.handle)
    }

    fn uuid(&self) -> Result<Uuid> {
        Ok(self.uuid)
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    fn visible_bounds(&self) -> Bounds<Pixels> {
        self.visible_bounds
    }
}

pub(crate) fn logical_point(x: f32, y: f32, scale_factor: f32) -> Point<Pixels> {
    point(px(x / scale_factor), px(y / scale_factor))
}

fn logical_bounds(rect: RECT, scale_factor: f32) -> Bounds<Pixels> {
    Bounds {
        origin: logical_point(rect.left as f32, rect.top as f32, scale_factor),
        size: size(
            px((rect.right - rect.left) as f32 / scale_factor),
            px((rect.bottom - rect.top) as f32 / scale_factor),
        ),
    }
}

pub(crate) fn display_id_for_monitor(monitor: HMONITOR) -> DisplayId {
    DisplayId::new(monitor.0 as u64)
}

fn monitor_from_display_id(display_id: DisplayId) -> HMONITOR {
    HMONITOR(u64::from(display_id) as _)
}

fn enumerate_monitors() -> Vec<HMONITOR> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let monitors = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        monitors.push(monitor);
        BOOL(1)
    }

    let mut monitors = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
        )
        .ok()
        .log_err();
    }
    monitors
}

fn monitor_info(monitor: HMONITOR) -> Result<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if !ok.as_bool() {
        bail!(std::io::Error::last_os_error());
    }
    Ok(info)
}

fn monitor_scale_factor(monitor: HMONITOR) -> Result<f32> {
    let (mut dpi_x, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }?;
    Ok(dpi_x as f32 / USER_DEFAULT_SCREEN_DPI as f32)
}

/// A stable identifier derived from the monitor's device name, the same one the native Windows platform persists.
fn uuid_for_device_name(device_name: &[u16]) -> Uuid {
    let name = device_name
        .iter()
        .flat_map(|unit| unit.to_be_bytes())
        .collect_vec();
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_name(name: &str) -> [u16; 32] {
        let mut buffer = [0u16; 32];
        for (slot, unit) in buffer.iter_mut().zip(name.encode_utf16()) {
            *slot = unit;
        }
        buffer
    }

    #[test]
    fn uuid_is_stable_per_device_name() {
        let first = uuid_for_device_name(&device_name(r"\\.\DISPLAY1"));
        assert_eq!(first, uuid_for_device_name(&device_name(r"\\.\DISPLAY1")));
        assert_ne!(first, uuid_for_device_name(&device_name(r"\\.\DISPLAY2")));
    }

    #[test]
    fn uuid_matches_the_native_windows_derivation() {
        let name = device_name(r"\\.\DISPLAY1");
        let bytes: Vec<u8> = name.iter().flat_map(|unit| unit.to_be_bytes()).collect();
        assert_eq!(
            uuid_for_device_name(&name),
            Uuid::new_v5(&Uuid::NAMESPACE_DNS, &bytes)
        );
    }

    #[test]
    fn display_id_round_trips_the_monitor_handle() {
        let monitor = HMONITOR(0x1234 as _);
        let id = display_id_for_monitor(monitor);
        assert_eq!(monitor_from_display_id(id), monitor);
    }

    #[test]
    fn logical_bounds_divide_by_the_scale_factor() {
        let rect = RECT {
            left: 1920,
            top: 0,
            right: 1920 + 3840,
            bottom: 2160,
        };
        let bounds = logical_bounds(rect, 2.0);
        assert_eq!(bounds.origin, point(px(960.0), px(0.0)));
        assert_eq!(bounds.size, size(px(1920.0), px(1080.0)));
    }
}
