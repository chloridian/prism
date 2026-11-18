//! The wgpu renderer behind a window, built on the device every window shares.

use std::sync::Arc;

use anyhow::Result;
use gpui::{DevicePixels, GpuSpecs, Scene, Size};
use gpui_wgpu::{
    FontRasterizationSettings, GpuContext, SubpixelOrder, WgpuAtlas, WgpuContextHandle,
    WgpuDeviceRequirements, WgpuRenderer, WgpuSurfaceConfig, wgpu,
};
use windows::{
    Win32::Graphics::DirectWrite::{
        DWRITE_FACTORY_TYPE_SHARED, DWRITE_PIXEL_GEOMETRY_BGR, DWriteCreateFactory,
        IDWriteFactory5, IDWriteRenderingParams1,
    },
    core::Interface as _,
};
use winit::window::Window;

/// The device every window shares, and what the application requires of it.
#[derive(Clone, Default)]
pub(crate) struct RendererContext {
    gpu: GpuContext,
    requirements: Option<WgpuDeviceRequirements>,
}

impl RendererContext {
    pub(crate) fn set_requirements(&mut self, requirements: WgpuDeviceRequirements) {
        self.requirements = Some(requirements);
    }
}

pub(crate) struct Renderer {
    renderer: WgpuRenderer,
    window: Arc<Window>,
}

impl Renderer {
    pub(crate) fn new(
        context: RendererContext,
        window: Arc<Window>,
        size: Size<DevicePixels>,
    ) -> Result<Self> {
        let config = WgpuSurfaceConfig {
            size: Size {
                width: DevicePixels(size.width.0.max(1)),
                height: DevicePixels(size.height.0.max(1)),
            },
            transparent: false,
            preferred_present_mode: None,
        };
        let mut renderer =
            WgpuRenderer::new(context.gpu, &window, config, None, context.requirements)?;
        renderer.set_font_rasterization_settings(font_rasterization_settings()?);
        Ok(Self { renderer, window })
    }

    pub(crate) fn sprite_atlas(&self) -> &Arc<WgpuAtlas> {
        self.renderer.sprite_atlas()
    }

    pub(crate) fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.renderer.update_drawable_size(size);
    }

    pub(crate) fn update_transparency(&mut self, transparent: bool) {
        self.renderer.update_transparency(transparent);
    }

    /// Draws `scene`, recovering the device first if it was lost. Returns
    /// whether the next frame must render the scene again, uncached.
    pub(crate) fn draw(&mut self, scene: &Scene) -> bool {
        if self.renderer.device_lost() {
            if let Err(error) = self.renderer.recover(&self.window) {
                log::warn!("GPU recovery failed, will retry on next frame: {error}");
            }
            return true;
        }
        self.renderer.draw(scene);
        self.renderer.needs_redraw()
    }

    pub(crate) fn gpu_specs(&self) -> GpuSpecs {
        self.renderer.gpu_specs()
    }

    pub(crate) fn gpu_context(&self) -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
        self.renderer.gpu_context()
    }

    pub(crate) fn gpu_context_info(&self) -> Option<WgpuContextHandle> {
        self.renderer.gpu_context_info()
    }

    pub(crate) fn device_lost(&self) -> bool {
        self.renderer.device_lost()
    }
}

fn font_rasterization_settings() -> Result<FontRasterizationSettings> {
    let params: IDWriteRenderingParams1 = unsafe {
        let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        factory.CreateRenderingParams()?.cast()?
    };
    let order = if unsafe { params.GetPixelGeometry() } == DWRITE_PIXEL_GEOMETRY_BGR {
        SubpixelOrder::BlueGreenRed
    } else {
        SubpixelOrder::RedGreenBlue
    };
    Ok(FontRasterizationSettings::new(
        unsafe { params.GetGamma() },
        unsafe { params.GetGrayscaleEnhancedContrast() },
        unsafe { params.GetEnhancedContrast() },
        order,
    ))
}
