//! Guards the shared shader boundary of the WGPU renderer.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("gpui crate must live at <workspace>/crates/gpui")
        .to_path_buf()
}

fn collect_renderer_shader_sources(root: &Path, sources: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!(
                "failed to inspect an entry under {}: {error}",
                root.display()
            )
        });
        let path = entry.path();
        let file_type = entry
            .file_type()
            .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()));

        if file_type.is_dir() {
            collect_renderer_shader_sources(&path, sources);
        } else if matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("wgsl" | "metal" | "hlsl")
        ) {
            sources.push(path);
        }
    }
}

fn relative_paths(root: &Path, paths: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    let mut relative: Vec<_> = paths
        .into_iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string()
        })
        .collect();
    relative.sort();
    relative
}

#[test]
fn renderer_backends_share_rust_authored_shaders() {
    let root = workspace_root();

    let mut platform_shader_sources = Vec::new();
    for relative in [
        "crates/gpui_apple/src",
        "crates/gpui_macos/src",
        "crates/gpui_windows/src",
    ] {
        let path = root.join(relative);
        if path.exists() {
            collect_renderer_shader_sources(&path, &mut platform_shader_sources);
        }
    }
    assert!(
        platform_shader_sources.is_empty(),
        "platform-local renderer shader source bypasses shared generation: {}",
        relative_paths(&root, platform_shader_sources).join(", ")
    );

    let rust_shader_module = root.join("crates/gpui_render/src/shaders/mod.rs");
    assert!(
        rust_shader_module.is_file(),
        "renderers must keep their typed Rust shader module in gpui_render"
    );

    let mut standalone_shader_sources = Vec::new();
    for relative in ["crates/gpui_render/src", "crates/gpui_wgpu/src"] {
        collect_renderer_shader_sources(&root.join(relative), &mut standalone_shader_sources);
    }
    assert!(
        standalone_shader_sources.is_empty(),
        "standalone shader sources bypass typed generation: {}",
        relative_paths(&root, standalone_shader_sources).join(", ")
    );
}

#[test]
fn native_renderer_fallbacks_and_intermediates_are_lazy() {
    let root = workspace_root();
    let wgpu_context_path = root.join("crates/gpui_wgpu/src/wgpu_context.rs");
    let wgpu_context = fs::read_to_string(&wgpu_context_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", wgpu_context_path.display()));
    assert!(
        wgpu_context.contains("enum NativeBackend"),
        "{} must represent one native backend as a closed type",
        wgpu_context_path.display()
    );
    assert!(
        wgpu_context.contains("impl From<NativeBackend> for wgpu::Backends")
            && wgpu_context.contains("backends: self.into()")
            && wgpu_context.contains("try_in_preference_order"),
        "{} must initialize one backend per fallback attempt",
        wgpu_context_path.display()
    );
    assert!(
        !wgpu_context.contains("Backends::VULKAN | wgpu::Backends::GL"),
        "{} must not eagerly construct the GL fallback beside Vulkan",
        wgpu_context_path.display()
    );
}
