//! Text clipboard access through arboard.

use arboard::{Clipboard, Error};
use gpui::ClipboardItem;

pub(crate) fn read() -> Option<ClipboardItem> {
    let text = match Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
        Ok(text) => text,
        Err(Error::ContentNotAvailable) => return None,
        Err(error) => {
            log::warn!("failed to read the clipboard: {error}");
            return None;
        }
    };
    (!text.is_empty()).then(|| ClipboardItem::new_string(text))
}

pub(crate) fn write(item: &ClipboardItem) {
    let Some(text) = item.text() else {
        return;
    };
    if let Err(error) = Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
        log::error!("failed to write the clipboard: {error}");
    }
}
