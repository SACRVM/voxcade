//! A window to draw into: SDL3's window, its wgpu surface and the GPU
//! behind it, set up the same for the game (`app`) and the editor.

use anyhow::{Context, Result, bail};

use crate::render::{FrameData, Renderer};

/// The GPU windows draw with: one for all the windows of a program (the
/// IDE's and its editors'), each holding its own handles to it.
#[derive(Clone)]
pub struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// A window and what draws into it. The surface comes first: fields drop
/// in the order they are declared, and the surface must go before the
/// window it was made from.
pub struct Screen {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub window: sdl3::video::Window,
    gpu: Gpu,
}

impl Screen {
    /// A resizable window `width` by `height` (points) in the middle of the
    /// screen. `quick`: frames shown as soon as they are ready (the game);
    /// otherwise in step with the display (the editor, which draws only
    /// when something changed).
    pub fn open(video: &sdl3::VideoSubsystem, title: &str, width: u32, height: u32, quick: bool) -> Result<Self> {
        Self::open_on(video, title, width, height, quick, None)
    }

    /// The same, drawn with `gpu` (another window's) if given, else with a
    /// GPU of its own.
    pub fn open_on(video: &sdl3::VideoSubsystem, title: &str, width: u32, height: u32, quick: bool, gpu: Option<&Gpu>) -> Result<Self> {
        let mut window = video.window(title, width, height).position_centered().resizable().high_pixel_density().build().context("creating window")?;
        crate::desktop::set_icon(&mut window);
        let instance = gpu.map_or_else(|| wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle()), |g| g.instance.clone());
        // SAFETY: the surface is dropped before the window (`Screen`'s
        // field order).
        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_display_and_window(&window, &window).context("window handle")?;
            instance.create_surface_unsafe(target).context("creating surface")?
        };
        let gpu = match gpu {
            Some(g) => g.clone(),
            None => {
                let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .context("no suitable GPU")?;
                let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("voxcade"),
                    ..Default::default()
                }))
                .context("creating GPU device")?;
                Gpu { instance, adapter, device, queue }
            }
        };
        let (adapter, device, queue) = (&gpu.adapter, gpu.device.clone(), gpu.queue.clone());

        let caps = surface.get_capabilities(adapter);
        // Non-sRGB target: lighting happens in palette space, like the old machines.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let present_mode = if quick && caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let (w, h) = window.size_in_pixels();
        let mut config = surface.get_default_config(adapter, w, h).context("surface unsupported")?;
        config.format = format;
        config.present_mode = present_mode;
        config.desired_maximum_frame_latency = 1;
        surface.configure(&device, &config);
        Ok(Self { surface, device, queue, config, window, gpu })
    }

    /// The GPU it draws with, for other windows to share.
    pub fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    /// The window's new size in pixels; the renderer follows.
    pub fn resize(&mut self, renderer: &mut Renderer, width: i32, height: i32) {
        self.resize_surface(width, height);
        renderer.resize(&self.device, self.config.width, self.config.height);
    }

    /// The window's new size in pixels, for the surface alone.
    pub fn resize_surface(&mut self, width: i32, height: i32) {
        self.config.width = width.max(1) as u32;
        self.config.height = height.max(1) as u32;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    /// A point of the window (in its points, as the mouse gives them) as
    /// shares of its width and height, from the top left.
    pub fn share(&self, x: f32, y: f32) -> (f32, f32) {
        let (w, h) = self.window.size();
        (x / w.max(1) as f32, y / h.max(1) as f32)
    }

    /// Draws a frame and shows it.
    pub fn show(&mut self, renderer: &mut Renderer, frame: &FrameData) -> Result<()> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                renderer.render(&self.device, &self.queue, &mut encoder, &view, frame);
                self.queue.submit([encoder.finish()]);
                self.queue.present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => self.surface.configure(&self.device, &self.config),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => bail!("surface validation error"),
        }
        Ok(())
    }
}
