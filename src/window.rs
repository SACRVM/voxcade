//! A window to draw into: SDL3's window, its wgpu surface and the GPU
//! behind it, set up the same for the game (`app`) and the editor.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::settings::FrameRate;

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

impl Gpu {
    /// The adapter (its kind says whether the graphics are the processor's own).
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }
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
        // Non-sRGB where there is one; else the renderer decodes for sRGB.
        let format = crate::render::surface_format(&caps.formats).context("the window's surface offers no format to draw in")?;
        let present_mode = present_mode(&caps.present_modes, if quick { FrameRate::Unlimited } else { FrameRate::Vsync });
        let (w, h) = window.size_in_pixels();
        let mut config = surface.get_default_config(adapter, w, h).context("surface unsupported")?;
        config.format = format;
        config.present_mode = present_mode;
        config.desired_maximum_frame_latency = 1;
        // Readable, so another session driving the IDE can be handed a
        // picture of a window (`Screen::picture`).
        config.usage |= caps.usages & wgpu::TextureUsages::COPY_SRC;
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

    /// The window's frame in `texture` (the surface's, drawn but not yet
    /// presented) as PNG bytes.
    pub fn picture(&self, texture: &wgpu::Texture) -> Result<Vec<u8>> {
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            bail!("this system's window cannot be read back");
        }
        use wgpu::TextureFormat as F;
        let format = self.config.format;
        if !matches!(format, F::Rgba8Unorm | F::Rgba8UnormSrgb | F::Bgra8Unorm | F::Bgra8UnormSrgb | F::Rgb10a2Unorm) {
            bail!("a window of format {format:?} cannot be read back");
        }
        let mut rgba = crate::headless::read_texture(&self.device, &self.queue, texture)?;
        for px in rgba.as_chunks_mut::<4>().0 {
            match format {
                F::Bgra8Unorm | F::Bgra8UnormSrgb => px.swap(0, 2),
                // Ten bits to a colour, red lowest.
                F::Rgb10a2Unorm => {
                    let v = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
                    for (i, c) in px[..3].iter_mut().enumerate() {
                        *c = (((v >> (10 * i)) & 0x3ff) >> 2) as u8;
                    }
                }
                _ => {}
            }
            px[3] = 255;
        }
        crate::headless::png_bytes(texture.width(), texture.height(), &rgba)
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

/// How the surface presents for a frame rate: UNLIMITED shows a picture
/// as soon as it is ready (Mailbox, where the system has it), every other
/// choice waits for the screen's refresh (Fifo, which every system has);
/// the capped ones then rest between pictures (`Pacer`).
pub fn present_mode(available: &[wgpu::PresentMode], rate: FrameRate) -> wgpu::PresentMode {
    if !rate.waits_for_the_screen() && available.contains(&wgpu::PresentMode::Mailbox) {
        wgpu::PresentMode::Mailbox
    } else {
        wgpu::PresentMode::Fifo
    }
}

/// Presents the surface the way `rate` asks, configured again only when
/// that changes the mode.
pub fn set_frame_rate(surface: &wgpu::Surface<'static>, device: &wgpu::Device, config: &mut wgpu::SurfaceConfiguration, adapter: &wgpu::Adapter, rate: FrameRate) {
    let mode = present_mode(&surface.get_capabilities(adapter).present_modes, rate);
    if config.present_mode != mode {
        config.present_mode = mode;
        surface.configure(device, config);
    }
}

/// Rests between pictures so the game is drawn at most `limit` times a
/// second (`FrameRate::limit`): it sleeps till the next picture is due,
/// and starts the count anew when the machine fell behind.
#[derive(Default)]
pub struct Pacer {
    due: Option<Instant>,
}

impl Pacer {
    /// How long to rest at `now` for `limit` pictures a second, and when
    /// the next one is due; none: no limit, the count starts anew.
    pub fn rest(&mut self, now: Instant, limit: Option<u32>) -> Duration {
        let Some(limit) = limit.filter(|&l| l > 0) else {
            self.due = None;
            return Duration::ZERO;
        };
        let next = self.due.unwrap_or(now) + Duration::from_secs_f64(1.0 / limit as f64);
        if next <= now {
            self.due = Some(now);
            Duration::ZERO
        } else {
            self.due = Some(next);
            next - now
        }
    }

    /// Rests as `rest` says.
    pub fn wait(&mut self, limit: Option<u32>) {
        let rest = self.rest(Instant::now(), limit);
        if !rest.is_zero() {
            std::thread::sleep(rest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only UNLIMITED shows pictures as they are ready; the rest wait for
    /// the screen; without Mailbox it waits too.
    #[test]
    fn the_frame_rate_picks_the_present_mode() {
        use wgpu::PresentMode::{Fifo, Mailbox};
        assert_eq!(present_mode(&[Fifo, Mailbox], FrameRate::Unlimited), Mailbox);
        assert_eq!(present_mode(&[Fifo, Mailbox], FrameRate::Vsync), Fifo);
        assert_eq!(present_mode(&[Fifo, Mailbox], FrameRate::Fps60), Fifo);
        assert_eq!(present_mode(&[Fifo], FrameRate::Unlimited), Fifo);
    }

    /// 60 a second is a rest of a 60th of a second at first, then what is
    /// left of each period; a late frame does not rest, nor pile up debt;
    /// no limit never rests.
    #[test]
    fn the_pacer_keeps_to_the_limit() {
        let mut pacer = Pacer::default();
        let t0 = Instant::now();
        let period = Duration::from_secs_f64(1.0 / 60.0);
        assert_eq!(pacer.rest(t0, Some(60)), period);
        let half = pacer.rest(t0 + period / 2, Some(60));
        assert!(half > period && half < period * 2, "the second period, less the half spent: {half:?}");
        let late = t0 + Duration::from_secs(5);
        assert_eq!(pacer.rest(late, Some(60)), Duration::ZERO, "behind: no rest");
        assert_eq!(pacer.rest(late, Some(60)), period, "and the count starts at once");
        assert_eq!(pacer.rest(late, None), Duration::ZERO);
        assert_eq!(pacer.rest(late, Some(30)).as_millis(), 33, "a new limit starts afresh");
    }
}
