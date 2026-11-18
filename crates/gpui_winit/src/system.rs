//! Operating system services outside windows and input: shell operations, appearance and settings.

use std::ffi::OsStr;

use anyhow::{Context as _, Result};
use gpui::{CursorStyle, WindowAppearance};
use windows::{
    UI::{
        Color,
        ViewManagement::{UIColorType, UISettings},
    },
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, HWND},
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
        UI::{
            Shell::{SHGetDesktopFolder, SHOpenFolderAndSelectItems, ShellExecuteW},
            WindowsAndMessaging::SW_SHOWDEFAULT,
        },
    },
    core::{HSTRING, w},
};
use winit::window::CursorIcon;

pub(crate) fn cursor_icon(style: CursorStyle) -> CursorIcon {
    match style {
        CursorStyle::Arrow => CursorIcon::Default,
        CursorStyle::IBeam => CursorIcon::Text,
        CursorStyle::Crosshair => CursorIcon::Crosshair,
        CursorStyle::ClosedHand => CursorIcon::Grabbing,
        CursorStyle::OpenHand => CursorIcon::Grab,
        CursorStyle::PointingHand => CursorIcon::Pointer,
        CursorStyle::ResizeLeft => CursorIcon::WResize,
        CursorStyle::ResizeRight => CursorIcon::EResize,
        CursorStyle::ResizeLeftRight => CursorIcon::EwResize,
        CursorStyle::ResizeUp => CursorIcon::NResize,
        CursorStyle::ResizeDown => CursorIcon::SResize,
        CursorStyle::ResizeUpDown => CursorIcon::NsResize,
        CursorStyle::ResizeUpLeft => CursorIcon::NwResize,
        CursorStyle::ResizeUpRight => CursorIcon::NeResize,
        CursorStyle::ResizeDownLeft => CursorIcon::SwResize,
        CursorStyle::ResizeDownRight => CursorIcon::SeResize,
        CursorStyle::ResizeUpLeftDownRight => CursorIcon::NwseResize,
        CursorStyle::ResizeUpRightDownLeft => CursorIcon::NeswResize,
        CursorStyle::ResizeColumn => CursorIcon::ColResize,
        CursorStyle::ResizeRow => CursorIcon::RowResize,
        CursorStyle::IBeamCursorForVerticalLayout => CursorIcon::VerticalText,
        CursorStyle::OperationNotAllowed => CursorIcon::NotAllowed,
        CursorStyle::DragLink => CursorIcon::Alias,
        CursorStyle::DragCopy => CursorIcon::Copy,
        CursorStyle::ContextualMenu => CursorIcon::ContextMenu,
    }
}

/// Initializes COM for the calling thread, which the WinRT settings and shell calls below need.
pub(crate) fn initialize_com() {
    let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if result.is_err() {
        log::warn!("COM initialization failed: {result:?}");
    }
}

pub(crate) fn appearance() -> Result<WindowAppearance> {
    let settings = UISettings::new()?;
    let foreground = settings.GetColorValue(UIColorType::Foreground)?;
    // A light foreground means the system draws dark windows.
    Ok(if is_light(&foreground) {
        WindowAppearance::Dark
    } else {
        WindowAppearance::Light
    })
}

fn is_light(color: &Color) -> bool {
    5 * u32::from(color.G) + 2 * u32::from(color.R) + u32::from(color.B) > 8 * 128
}

pub(crate) fn should_auto_hide_scrollbars() -> Result<bool> {
    Ok(UISettings::new()?.AutoHideScrollBars()?)
}

pub(crate) fn open(target: impl AsRef<OsStr>) -> Result<()> {
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(target.as_ref()),
            None,
            None,
            SW_SHOWDEFAULT,
        )
    };
    if result.0 as isize <= 32 {
        anyhow::bail!("unable to open target: {}", std::io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn reveal(target: &std::path::Path) -> Result<()> {
    let dir = target.parent().context("no parent folder found")?;
    let desktop = unsafe { SHGetDesktopFolder()? };

    let mut dir_item = std::ptr::null_mut();
    unsafe {
        desktop.ParseDisplayName(
            HWND::default(),
            None,
            &HSTRING::from(dir),
            None,
            &mut dir_item,
            std::ptr::null_mut(),
        )?;
    }
    let mut file_item = std::ptr::null_mut();
    unsafe {
        desktop.ParseDisplayName(
            HWND::default(),
            None,
            &HSTRING::from(target),
            None,
            &mut file_item,
            std::ptr::null_mut(),
        )?;
    }

    let highlight = [file_item as *const _];
    unsafe { SHOpenFolderAndSelectItems(dir_item as _, Some(&highlight), 0) }.or_else(|error| {
        if error.code().0 == ERROR_FILE_NOT_FOUND.0 as i32 {
            // On some systems the call fails with "file not found" even though the file exists.
            // ShellExecute still opens the folder, without selecting the file.
            open(dir).context("opening the parent folder")
        } else {
            Err(anyhow::anyhow!("cannot reveal the path: {error}"))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_foregrounds_mean_dark_mode() {
        let white = Color {
            A: 255,
            R: 255,
            G: 255,
            B: 255,
        };
        let black = Color {
            A: 255,
            R: 0,
            G: 0,
            B: 0,
        };
        assert!(is_light(&white));
        assert!(!is_light(&black));
    }
}
