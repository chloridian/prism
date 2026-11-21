use super::*;
use anyhow::Result;
use collections::FxHashMap;
use windows_061::{
    Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Texture2D},
    core::Interface as _,
};

#[path = "windows/shared.rs"]
mod shared;
#[path = "windows/upload.rs"]
mod upload;

use shared::SharedTexture;
use upload::UploadedTexture;

pub(in crate::wgpu_renderer) struct SurfaceCache {
    textures: FxHashMap<CaptureFrameKey, CachedTexture>,
    /// Bindings for application-rendered WGPU textures.
    application_textures: FxHashMap<wgpu::Texture, SurfaceBinding>,
}

impl SurfaceCache {
    pub(in crate::wgpu_renderer) fn new(_device: &wgpu::Device) -> Result<Self> {
        Ok(Self {
            textures: FxHashMap::default(),
            application_textures: FxHashMap::default(),
        })
    }
}

pub(super) fn retain_surface_cache(renderer: &WgpuRenderer, surfaces: &[PaintSurface]) {
    let active_keys = surfaces
        .iter()
        .filter_map(|surface| capture_frame(surface).map(CaptureFrameKey::from_frame))
        .collect::<smallvec::SmallVec<[CaptureFrameKey; 4]>>();
    let active_textures = surfaces
        .iter()
        .filter_map(|surface| match &surface.source {
            gpui::SurfaceSource::Texture { texture, .. } => texture.downcast_ref::<wgpu::Texture>(),
            _ => None,
        })
        .collect::<smallvec::SmallVec<[&wgpu::Texture; 4]>>();
    let mut cache = renderer.resources().surface_cache.borrow_mut();
    cache.textures.retain(|key, _| active_keys.contains(key));
    cache
        .application_textures
        .retain(|texture, _| active_textures.contains(&texture));
}

pub(super) fn draw_surfaces(
    renderer: &WgpuRenderer,
    surfaces: &[PaintSurface],
    pass: &mut wgpu::RenderPass<'_>,
) -> frame::DrawResult {
    let resources = renderer.resources();
    let mut cache = resources.surface_cache.borrow_mut();
    for surface in surfaces {
        if let gpui::SurfaceSource::Texture { texture, .. } = &surface.source {
            let Some(texture) = texture.downcast_ref::<wgpu::Texture>() else {
                log::error!("surface source is not a WGPU texture");
                return Err(frame::DrawError::ExternalSurface);
            };
            let binding = cache
                .application_textures
                .entry(texture.clone())
                .or_insert_with(|| {
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    SurfaceBinding::new(renderer, view.clone(), view)
                });
            renderer.draw_surface_binding(surface, SurfaceColorFormat::Rgba, binding, pass)?;
            continue;
        }
        let Some(frame) = capture_frame(surface) else {
            log::error!("surface source cannot be imported by the Windows renderer");
            return Err(frame::DrawError::ExternalSurface);
        };
        let source = frame.texture();
        let key = CaptureFrameKey::from_frame(frame);
        let size = source_size(source);
        if cache
            .textures
            .get(&key)
            .is_none_or(|cached| cached.size != size)
        {
            cache.textures.insert(
                key,
                CachedTexture::new(renderer, source).map_err(surface_error)?,
            );
        }

        let Some(cached) = cache.textures.get_mut(&key) else {
            log::error!("capture texture cache insertion failed");
            return Err(frame::DrawError::ExternalSurface);
        };
        // A frame can be painted more than once (for clipping, filters, or repeated content).
        // The cache key includes the capture timestamp, so each frame gets its own destination;
        // `updated` prevents duplicate GPU copies/uploads for repeated uses of that frame.
        if !cached.updated {
            if let Err(error) = cached.update(&resources.queue, source) {
                if !cached.is_shared() {
                    return Err(surface_error(error));
                }
                log::warn!("shared capture update failed, switching to CPU upload: {error:#}");
                *cached = CachedTexture::new_uploaded(renderer, source)
                    .and_then(|mut cached| {
                        cached.update(&resources.queue, source)?;
                        Ok(cached)
                    })
                    .map_err(surface_error)?;
            }
            cached.updated = true;
        }

        renderer.draw_surface_binding(
            surface,
            SurfaceColorFormat::Rgba,
            &mut cached.binding,
            pass,
        )?;
    }
    Ok(())
}

fn capture_frame(surface: &PaintSurface) -> Option<&gpui::WindowsScreenCaptureFrame> {
    let gpui::SurfaceSource::WindowsCapture(frame) = &surface.source else {
        return None;
    };
    Some(frame)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CaptureFrameKey {
    texture: usize,
    display_time: u64,
}

impl CaptureFrameKey {
    fn from_frame(frame: &gpui::WindowsScreenCaptureFrame) -> Self {
        Self {
            texture: frame.texture().as_raw() as usize,
            display_time: frame.display_time(),
        }
    }
}

fn surface_error(error: anyhow::Error) -> frame::DrawError {
    log::error!("failed to import Windows capture frame: {error:#}");
    frame::DrawError::ExternalSurface
}

struct CachedTexture {
    size: wgpu::Extent3d,
    updated: bool,
    backend: CaptureBackend,
    _texture: wgpu::Texture,
    binding: SurfaceBinding,
}

impl CachedTexture {
    fn new(renderer: &WgpuRenderer, source: &ID3D11Texture2D) -> Result<Self> {
        let device = &renderer.resources().device;
        if let Some(hal_device) = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() } {
            match SharedTexture::new(device, &hal_device, source) {
                Ok((backend, texture)) => {
                    return Ok(Self::from_backend(renderer, backend.into(), texture));
                }
                Err(error) => {
                    log::warn!("D3D12 capture sharing is unavailable, using CPU upload: {error:#}");
                }
            }
        }
        Self::new_uploaded(renderer, source)
    }

    fn new_uploaded(renderer: &WgpuRenderer, source: &ID3D11Texture2D) -> Result<Self> {
        let (backend, texture) = UploadedTexture::new(&renderer.resources().device, source)?;
        Ok(Self::from_backend(renderer, backend.into(), texture))
    }

    fn from_backend(
        renderer: &WgpuRenderer,
        backend: CaptureBackend,
        texture: wgpu::Texture,
    ) -> Self {
        let size = backend.size();
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            size,
            updated: false,
            backend,
            binding: SurfaceBinding::new(renderer, view.clone(), view),
            _texture: texture,
        }
    }

    fn update(&mut self, queue: &wgpu::Queue, source: &ID3D11Texture2D) -> Result<()> {
        match &mut self.backend {
            CaptureBackend::Shared(texture) => texture.update(source),
            CaptureBackend::Uploaded(texture) => texture.update(queue, source),
        }
    }

    fn is_shared(&self) -> bool {
        matches!(self.backend, CaptureBackend::Shared(_))
    }
}

enum CaptureBackend {
    Shared(SharedTexture),
    Uploaded(UploadedTexture),
}

impl CaptureBackend {
    fn size(&self) -> wgpu::Extent3d {
        match self {
            Self::Shared(texture) => texture.size,
            Self::Uploaded(texture) => texture.size,
        }
    }
}

impl From<SharedTexture> for CaptureBackend {
    fn from(value: SharedTexture) -> Self {
        Self::Shared(value)
    }
}

impl From<UploadedTexture> for CaptureBackend {
    fn from(value: UploadedTexture) -> Self {
        Self::Uploaded(value)
    }
}

fn source_descriptor(source: &ID3D11Texture2D) -> D3D11_TEXTURE2D_DESC {
    let mut descriptor = D3D11_TEXTURE2D_DESC::default();
    unsafe { source.GetDesc(&mut descriptor) };
    descriptor
}

fn source_size(source: &ID3D11Texture2D) -> wgpu::Extent3d {
    capture_size(&source_descriptor(source))
}

fn validate_capture_descriptor(descriptor: &D3D11_TEXTURE2D_DESC) -> Result<()> {
    use windows_061::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
    };

    anyhow::ensure!(
        descriptor.Width > 0 && descriptor.Height > 0,
        "capture texture has empty dimensions"
    );
    anyhow::ensure!(
        descriptor.MipLevels == 1 && descriptor.ArraySize == 1 && descriptor.SampleDesc.Count == 1,
        "capture texture layout is unsupported"
    );
    anyhow::ensure!(
        descriptor.Format == DXGI_FORMAT_B8G8R8A8_UNORM
            || descriptor.Format == DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        "capture texture format {} is not BGRA8",
        descriptor.Format.0
    );
    Ok(())
}

fn capture_size(descriptor: &D3D11_TEXTURE2D_DESC) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: descriptor.Width,
        height: descriptor.Height,
        depth_or_array_layers: 1,
    }
}

fn capture_texture_descriptor(
    label: &'static str,
    size: wgpu::Extent3d,
) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // Windows Graphics Capture produces DXGI_FORMAT_B8G8R8A8_UNORM. Importing it as
        // sRGB applies a second decode during sampling and visibly darkens captured content.
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    }
}

#[cfg(test)]
mod tests {
    use collections::FxHashMap;

    use super::CaptureFrameKey;

    #[test]
    fn capture_frame_key_controls_cache_identity_and_pruning() {
        let first = CaptureFrameKey {
            texture: 42,
            display_time: 1,
        };
        let second = CaptureFrameKey {
            texture: 42,
            display_time: 2,
        };
        assert_ne!(first, second);

        let mut cache = FxHashMap::default();
        assert_eq!(cache.insert(first, 1), None);
        assert_eq!(cache.insert(second, 2), None);
        assert_eq!(cache.insert(first, 3), Some(1));
        assert_eq!(cache.len(), 2);

        cache.retain(|key, _| *key == second);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&second), Some(&2));
    }
}

#[cfg(all(test, feature = "test-support"))]
mod application_texture_tests {
    use std::sync::Arc;

    use gpui::{
        Bounds, ContentMask, Corners, DevicePixels, PaintSurface, Point, ScaledPixels, Scene, Size,
        SurfaceSource,
    };

    use super::*;
    use crate::WgpuContext;

    const RED: [u8; 4] = [255, 0, 0, 255];

    fn renderer() -> anyhow::Result<WgpuRenderer> {
        let context = WgpuContext::new_headless(None)?;
        WgpuRenderer::new_headless(
            &context,
            Size {
                width: DevicePixels(4),
                height: DevicePixels(2),
            },
        )
    }

    fn solid_texture(renderer: &WgpuRenderer, color: [u8; 4]) -> Arc<wgpu::Texture> {
        let (device, queue) = renderer.gpu_context();
        let size = wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("application_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &color.repeat(4),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(2),
            },
            size,
        );
        Arc::new(texture)
    }

    fn scene(sources: impl IntoIterator<Item = SurfaceSource>) -> Scene {
        let bounds = |width| Bounds {
            origin: Point {
                x: ScaledPixels(0.0),
                y: ScaledPixels(0.0),
            },
            size: Size {
                width: ScaledPixels(width),
                height: ScaledPixels(2.0),
            },
        };
        let mut scene = Scene::default();
        for (order, source) in sources.into_iter().enumerate() {
            scene.insert_primitive(PaintSurface {
                order: order as u32,
                bounds: bounds(2.0),
                content_mask: ContentMask {
                    bounds: bounds(4.0),
                    ..Default::default()
                },
                corner_radii: Corners::default(),
                source,
            });
        }
        scene.finish();
        scene
    }

    fn texture_source(texture: &Arc<wgpu::Texture>) -> SurfaceSource {
        SurfaceSource::Texture {
            texture: texture.clone(),
            size: Size {
                width: DevicePixels(2),
                height: DevicePixels(2),
            },
        }
    }

    #[test]
    fn application_textures_composite_within_their_bounds() -> anyhow::Result<()> {
        let mut renderer = renderer()?;
        let clear = renderer.render_to_image(&scene([]))?.get_pixel(3, 1).0;
        assert_ne!(clear, RED);
        let texture = solid_texture(&renderer, RED);

        let image = renderer.render_to_image(&scene([texture_source(&texture)]))?;

        assert_eq!(image.get_pixel(0, 0).0, RED);
        assert_eq!(image.get_pixel(1, 1).0, RED);
        assert_eq!(image.get_pixel(2, 0).0, clear);
        assert_eq!(image.get_pixel(3, 1).0, clear);
        Ok(())
    }
}
