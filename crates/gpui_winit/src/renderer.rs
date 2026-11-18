//! The wgpu renderer behind a window, built on the device every window shares.

use std::{ffi::c_void, sync::Arc};

use anyhow::{Result, bail};
use gpui::{DevicePixels, GpuSpecs, Scene, Size};
use gpui_wgpu::{
    FontRasterizationSettings, GpuContext, SubpixelOrder, WgpuAtlas, WgpuContextHandle,
    WgpuDeviceRequirements, WgpuRenderer, WgpuSurfaceConfig, wgpu,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    Win32WindowHandle, WindowHandle,
};
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

/// The handle of a window, without owning it.
///
/// The first renderer hands its window to the shared wgpu instance as the display handle source.
/// The instance outlives every window, so a handle that owned the window would keep it alive
/// after it closed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowHandles {
    window: Win32WindowHandle,
}

impl WindowHandles {
    pub(crate) fn new(window: &Window) -> Result<Self> {
        match window.window_handle()?.as_raw() {
            RawWindowHandle::Win32(window) => Ok(Self { window }),
            other => bail!("unexpected window handle {other:?}"),
        }
    }

    pub(crate) fn hwnd(&self) -> HWND {
        HWND(self.window.hwnd.get() as *mut c_void)
    }
}

impl HasWindowHandle for WindowHandles {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the renderer drops before the window the handle names.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(self.window)) })
    }
}

impl HasDisplayHandle for WindowHandles {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

pub(crate) struct Renderer {
    renderer: WgpuRenderer,
    handles: WindowHandles,
}

impl Renderer {
    pub(crate) fn new(
        context: RendererContext,
        handles: WindowHandles,
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
            WgpuRenderer::new(context.gpu, &handles, config, None, context.requirements)?;
        renderer.set_font_rasterization_settings(font_rasterization_settings()?);
        Ok(Self { renderer, handles })
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
            if let Err(error) = self.renderer.recover(&self.handles) {
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
