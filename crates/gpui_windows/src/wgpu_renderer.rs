//! The WGPU renderer behind the interface the Windows window draws through.

use std::{num::NonZeroIsize, sync::Arc};

use anyhow::Result;
use gpui::{DevicePixels, GpuSpecs, Scene, Size};
use gpui_wgpu::{
    FontRasterizationSettings, GpuContext, SubpixelOrder, WgpuAtlas, WgpuDeviceRequirements,
    WgpuRenderer, WgpuSurfaceConfig, wgpu,
};
use raw_window_handle as rwh;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::DirectWrite::{
            DWRITE_FACTORY_TYPE_SHARED, DWRITE_PIXEL_GEOMETRY_BGR, DWriteCreateFactory,
            IDWriteFactory5, IDWriteRenderingParams1,
        },
    },
    core::Interface as _,
};

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

#[derive(Clone, Copy, Debug)]
struct WindowHandle(NonZeroIsize);

impl rwh::HasWindowHandle for WindowHandle {
    fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
        let raw = rwh::RawWindowHandle::Win32(rwh::Win32WindowHandle::new(self.0));
        Ok(unsafe { rwh::WindowHandle::borrow_raw(raw) })
    }
}

impl rwh::HasDisplayHandle for WindowHandle {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        Ok(rwh::DisplayHandle::windows())
    }
}

pub(crate) struct Renderer {
    renderer: WgpuRenderer,
    window: WindowHandle,
}

impl Renderer {
    pub(crate) fn new(
        context: RendererContext,
        hwnd: HWND,
        size: Size<DevicePixels>,
    ) -> Result<Self> {
        let window = WindowHandle(
            NonZeroIsize::new(hwnd.0 as isize)
                .ok_or_else(|| anyhow::anyhow!("window handle is null"))?,
        );
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

    pub(crate) fn device_lost(&self) -> bool {
        self.renderer.device_lost()
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn render_to_image(&mut self, scene: &Scene) -> Result<image::RgbaImage> {
        self.renderer.render_to_image(scene)
    }

    #[cfg(all(test, not(feature = "test-support")))]
    pub(crate) fn render_to_image(&mut self, _scene: &Scene) -> Result<image::RgbaImage> {
        anyhow::bail!("rendering to an image needs the test-support feature")
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
