//! WGSL linked at Cargo build time from the Rust-authored shader modules.

pub const BASE_WGSL: &str = include_str!(concat!(env!("OUT_DIR"), "/gpui_base.wgsl"));
pub const SUBPIXEL_DUAL_SOURCE_WGSL: &str =
    include_str!(concat!(env!("OUT_DIR"), "/gpui_subpixel_dual_source.wgsl"));
/// Downlevel (WebGL2/GLES) dialect: scene arrays travel via an `rgba32uint` data texture.
pub const BASE_DOWNLEVEL_WGSL: &str =
    include_str!(concat!(env!("OUT_DIR"), "/gpui_base_downlevel.wgsl"));

#[derive(Clone, Copy)]
pub struct GeneratedBinding {
    pub binding: u32,
    pub visibility: u32,
    pub kind: GeneratedBindingKind,
}

#[derive(Clone, Copy)]
pub enum GeneratedBindingKind {
    Uniform(u64),
    StorageRead(u64),
    Texture2dFloat,
    FilteringSampler,
    /// Downlevel only: the `rgba32uint` scene-data texture.
    DataTexture,
    /// Downlevel only: the per-draw batch base uniform, bound with a dynamic offset.
    RangeUniform,
}

include!(concat!(env!("OUT_DIR"), "/shader_interface.rs"));

#[cfg(test)]
mod tests {
    use super::BASE_DOWNLEVEL_WGSL;

    fn generated_function<'a>(source: &'a str, name: &str) -> &'a str {
        let marker = format!("fn {name}");

        source
            .split_once(&marker)
            .and_then(|(_, source)| source.split_once("\n}"))
            .map(|(source, _)| source)
            .unwrap_or_else(|| panic!("generated {name} function must exist"))
    }

    #[test]
    fn downlevel_background_array_uses_host_element_stride() {
        let decoder = generated_function(BASE_DOWNLEVEL_WGSL, "dl_load_Background_impl");
        let element_stride = std::mem::size_of::<gpui::LinearColorStop>() / 4;
        let second_element_offset = 7 + element_stride;

        assert!(
            decoder.contains("base + 7u"),
            "Background decoder must load its first color stop at word 7"
        );
        assert!(
            decoder.contains(&format!("base + {second_element_offset}u")),
            "Background decoder must use the host LinearColorStop stride"
        );
        assert!(
            !decoder.contains("base + 17u)), dl_scene_word"),
            "Background decoder must not read padding as the second color stop"
        );
    }

    #[test]
    fn downlevel_quad_decoder_uses_host_dash_offsets_and_stride() {
        let decoder = generated_function(BASE_DOWNLEVEL_WGSL, "dl_load_Quad_impl");
        let loader = generated_function(BASE_DOWNLEVEL_WGSL, "dl_load_QUADS");
        let dash_length_word = std::mem::offset_of!(gpui::Quad, border_dashed_length) / 4;
        let dash_gap_word = std::mem::offset_of!(gpui::Quad, border_dashed_gap) / 4;
        let stride_words = std::mem::size_of::<gpui::Quad>() / 4;

        assert!(
            decoder.contains(&format!("base, {dash_length_word}u")),
            "Quad decoder must load border_dashed_length from its host offset"
        );
        assert!(
            decoder.contains(&format!("base, {dash_gap_word}u")),
            "Quad decoder must load border_dashed_gap from its host offset"
        );
        assert!(
            loader.contains(&format!("i * {stride_words}u")),
            "Quad decoder must advance by the host Quad stride"
        );
    }
}
