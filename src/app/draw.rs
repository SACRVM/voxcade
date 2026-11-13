//! The window's picture: the world's changed chunks sent, the frame built
//! (the game or the title, a menu or the console over it, the lines of a
//! network game or a failure at the top), drawn and shown.

use anyhow::{Result, bail};

use super::App;
use crate::game::Game;
use crate::menu::Page;
use crate::sim::DT;

impl<G: Game> App<G> {
    pub(super) fn draw(&mut self) -> Result<()> {
        let App { renderer, screen, game, mask, front, net, settings, finder, net_finder, text_input, prompt, clock, parting, .. } = self;
        let crate::window::Screen { surface, device, queue, config, window, .. } = screen;
        renderer.update_chunks(device, queue, game.world_mut(), mask);

        // The picture's shape: the game's own (bars round it), else the
        // window's.
        let aspect = game.picture_aspect(config.width as f32 / config.height as f32);
        renderer.set_shape(device, game.cfg.camera.aspect);
        let scene = front.scene();
        let menu = &mut front.menu;
        (menu.device, menu.pad_style) = (front.played_with, front.pad_kind);
        // What the menus show of the network game; the games found on the
        // LAN while JOIN lists them; the host's address typed as text.
        menu.net = net.as_ref().filter(|_| menu.is_open()).map(|n| n.view(&settings.language()));
        if menu.page() == Some(Page::Find) {
            if finder.is_none() {
                *finder = game.fingerprint().ok().and_then(|f| crate::network::Finder::new(f).map_err(|e| eprintln!("searching the network: {e:#}")).ok());
            }
            let mut found = finder.as_mut().map(|f| f.found(&settings.language())).unwrap_or_default();
            if net_finder.is_none() && menu.internet {
                *net_finder = crate::network::app_of(game).ok().map(|(app, version)| crate::network::NetFinder::new(app, version));
            }
            if let Some(n) = net_finder {
                found.extend(n.found());
            }
            menu.found = found;
        } else if finder.take().is_some() | net_finder.take().is_some() {
            menu.found.clear();
        }
        let typing = matches!(menu.page(), Some(Page::Join | Page::Code));
        if typing != front.typing_address {
            front.typing_address = typing;
            if typing {
                text_input.start(window);
            } else {
                text_input.stop(window);
            }
        }
        let menu = &front.menu;
        let mut frame = if prompt.showing() && !menu.is_open() {
            // The console in place of the HUD, as a menu would be, the game
            // pushed up above it.
            let blink = clock.fps_since.elapsed().as_millis() % 1000 < 600;
            let ui = &game.cfg.ui;
            let camera = crate::ui::camera(&crate::ui::Layout::of(ui, aspect), ui.slant, ui.depth as f32);
            let mut frame = game.compose(clock.accumulator / DT, aspect, scene, Some((crate::ui::prompt_panel(prompt, blink, ui, aspect), Vec::new(), camera)));
            frame.view_proj = glam::Mat4::from_translation(glam::Vec3::new(0.0, 2.0 * crate::ui::prompt_lift(prompt), 0.0)) * frame.view_proj;
            frame
        } else {
            game.menu_frame(menu, settings, clock.accumulator / DT, aspect, scene)
        };
        if front.attracting {
            crate::ui::press_start(&mut frame.ui, front.attract_time, &game.cfg.ui, aspect, &settings.language());
        }
        if settings.show_fps {
            crate::ui::fps(&mut frame.ui, clock.fps.round() as u32, &game.cfg.ui, aspect);
        }
        // The network's lines at the top: who went quiet, who paused; on
        // leaving, why.
        let mut said = net.as_mut().map(|n| n.notices(&settings.language())).unwrap_or_default();
        if let Some((why, at)) = parting {
            if at.elapsed().as_secs() < 6 {
                said.push(why.clone());
            } else {
                *parting = None;
            }
        }
        if !said.is_empty() {
            // Over a menu low, above its hint and the host's help.
            let low = menu.is_open().then(|| 14.0 + menu.net.as_ref().filter(|_| menu.page() == Some(Page::Pause)).map_or(0, |n| n.help.len()) as f32 * 9.0);
            crate::ui::notice(&mut frame.ui, &said, &game.cfg.ui, aspect, low);
        }
        // A picture the game set at another pitch or field of view
        // (`frame:camera`) may need faces the meshes left out: built anew
        // with them, once.
        let faces = game.face_mask();
        if faces != *mask {
            *mask = faces;
            renderer.rebuild_all(device, queue, game.world_mut(), mask);
        }

        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let view = texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                renderer.render(device, queue, &mut encoder, &view, &frame);
                queue.submit([encoder.finish()]);
                queue.present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, config);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => bail!("surface validation error"),
        }
        Ok(())
    }
}
