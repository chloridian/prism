//! The WGPU renderer behind the interface the macOS window draws through.
//! Applications can share the window's device through
//! `gpui_wgpu::WgpuContextHandle` and composite their own textures.

use std::sync::Arc;

use foreign_types::ForeignType as _;
use gpui::{DevicePixels, GpuSpecs, Scene, Size};
use gpui_wgpu::{
    GpuContext, WgpuAtlas, WgpuContextHandle, WgpuDeviceRequirements, WgpuRenderer,
    WgpuSurfaceConfig, wgpu,
};
use metal::{CAMetalLayer, MetalLayer, MetalLayerRef};
use objc2_quartz_core::{CAAutoresizingMask, CAMetalLayer as Objc2CAMetalLayer};

/// The device every window shares, and what the application requires of it.
#[derive(Clone, Default)]
pub struct Context {
    gpu: GpuContext,
    requirements: Option<WgpuDeviceRequirements>,
}

impl Context {
    pub fn set_requirements(&mut self, requirements: WgpuDeviceRequirements) {
        self.requirements = Some(requirements);
    }
}

pub type Renderer = MacWgpuRenderer;

pub fn new_renderer(
    context: Context,
    bounds: gpui::Size<f32>,
    transparent: bool,
) -> anyhow::Result<Renderer> {
    MacWgpuRenderer::new(context, bounds, transparent)
}

/// Creates the CAMetalLayer that backs a window's view. The renderer drawing
/// into it sets its device and pixel format.
fn new_window_layer(transparent: bool) -> MetalLayer {
    let layer = MetalLayer::new();
    // Support direct-to-display rendering if the window is not transparent
    // https://developer.apple.com/documentation/metal/managing-your-game-window-for-metal-in-macos
    layer.set_opaque(!transparent);
    // `metal::MetalLayer` is a CAMetalLayer retained by the Metal crate.
    // Reborrow its Objective-C object as the generated objc2 class to keep
    // selector encodings and the autoresizing mask type checked here.
    let layer_object = unsafe { &*(layer.as_ptr() as *const Objc2CAMetalLayer) };
    layer_object.setAllowsNextDrawableTimeout(false);
    layer_object.setNeedsDisplayOnBoundsChange(true);
    layer_object.setAutoresizingMask(
        CAAutoresizingMask::LayerWidthSizable | CAAutoresizingMask::LayerHeightSizable,
    );
    layer
}

pub struct MacWgpuRenderer {
    renderer: WgpuRenderer,
    layer: MetalLayer,
}

impl MacWgpuRenderer {
    fn new(context: Context, bounds: gpui::Size<f32>, transparent: bool) -> anyhow::Result<Self> {
        let layer = new_window_layer(transparent);
        let config = WgpuSurfaceConfig {
            // The view resizes this to device pixels once it knows its scale factor.
            size: Size {
                width: DevicePixels(bounds.width.max(1.0) as i32),
                height: DevicePixels(bounds.height.max(1.0) as i32),
            },
            transparent,
            preferred_present_mode: None,
        };
        let renderer =
            WgpuRenderer::new_for_metal_layer(context.gpu, &layer, config, context.requirements)?;
        Ok(Self { renderer, layer })
    }

    pub fn layer(&self) -> Option<&MetalLayerRef> {
        Some(&self.layer)
    }

    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer.as_ptr()
    }

    pub fn sprite_atlas(&self) -> &Arc<WgpuAtlas> {
        self.renderer.sprite_atlas()
    }

    pub fn set_presents_with_transaction(&mut self, presents_with_transaction: bool) {
        self.layer
            .set_presents_with_transaction(presents_with_transaction);
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.renderer.update_drawable_size(size);
    }

    /// The surface's alpha mode sets the layer's opacity whenever WGPU
    /// configures it, so it must not be set on the layer directly.
    pub fn update_transparency(&mut self, transparent: bool) {
        self.renderer.update_transparency(transparent);
    }

    pub fn destroy(&mut self) {
        self.renderer.destroy();
    }

    /// Draws `scene`, recovering the device first if it was lost. Returns
    /// whether the next frame must render the scene again, uncached.
    pub fn draw(&mut self, scene: &Scene) -> bool {
        if self.renderer.device_lost() {
            if let Err(error) = self.renderer.recover_metal_layer(&self.layer) {
                log::warn!("GPU recovery failed, will retry on next frame: {error}");
            }
            return true;
        }
        self.renderer.draw(scene);
        self.renderer.needs_redraw()
    }

    pub fn gpu_specs(&self) -> GpuSpecs {
        self.renderer.gpu_specs()
    }

    pub fn gpu_context(&self) -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
        self.renderer.gpu_context()
    }

    pub fn device_lost(&self) -> bool {
        self.renderer.device_lost()
    }

    pub fn gpu_context_info(&self) -> Option<WgpuContextHandle> {
        self.renderer.gpu_context_info()
    }

    #[cfg(feature = "test-support")]
    pub fn render_to_image(&mut self, scene: &Scene) -> anyhow::Result<image::RgbaImage> {
        self.renderer.render_to_image(scene)
    }

    // WGPU's offscreen rendering needs gpui_wgpu's test-support, which the
    // `test-support` feature enables.
    #[cfg(all(test, not(feature = "test-support")))]
    pub fn render_to_image(&mut self, _scene: &Scene) -> anyhow::Result<image::RgbaImage> {
        anyhow::bail!("rendering to an image needs the test-support feature")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparency_survives_resizing() {
        let mut renderer =
            MacWgpuRenderer::new(Context::default(), gpui::size(64.0, 64.0), false).unwrap();
        assert!(renderer.layer.is_opaque());

        renderer.update_transparency(true);
        assert!(!renderer.layer.is_opaque());
        renderer.update_drawable_size(gpui::size(DevicePixels(128), DevicePixels(96)));
        assert!(!renderer.layer.is_opaque());

        renderer.update_transparency(false);
        renderer.update_drawable_size(gpui::size(DevicePixels(64), DevicePixels(64)));
        assert!(renderer.layer.is_opaque());
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn renders_scenes_to_images() {
        let mut renderer =
            MacWgpuRenderer::new(Context::default(), gpui::size(32.0, 16.0), false).unwrap();
        // A red quad over the left half.
        let bounds = |width| gpui::Bounds {
            origin: gpui::point(gpui::ScaledPixels(0.0), gpui::ScaledPixels(0.0)),
            size: gpui::size(gpui::ScaledPixels(width), gpui::ScaledPixels(16.0)),
        };
        let mut scene = Scene::default();
        scene.insert_primitive(gpui::Quad {
            bounds: bounds(16.0),
            content_mask: gpui::ContentMask {
                bounds: bounds(32.0),
                ..Default::default()
            },
            background: gpui::solid_background(gpui::red()),
            ..Default::default()
        });
        scene.finish();

        let image = renderer.render_to_image(&scene).unwrap();

        assert_eq!(image.dimensions(), (32, 16));
        assert_eq!(image.get_pixel(8, 8).0, [255, 0, 0, 255]);
        assert_ne!(image.get_pixel(24, 8).0, [255, 0, 0, 255]);
    }
}
