use std::{
    borrow::Cow,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub struct DirAssets {
    base: PathBuf,
}

impl DirAssets {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }
}

impl AssetSource for DirAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(Some(Cow::Owned(fs::read(self.base.join(path))?)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Vec::new();
        collect_files(&self.base, &self.base.join(path), &mut paths)?;
        Ok(paths)
    }
}

fn collect_files(base: &Path, dir: &Path, paths: &mut Vec<SharedString>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(base, &path, paths)?;
        } else if let Ok(relative) = path.strip_prefix(base) {
            paths.push(relative.to_string_lossy().replace('\\', "/").into());
        }
    }
    Ok(())
}
