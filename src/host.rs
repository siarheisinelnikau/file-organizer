//! Reactive Windows host: CPU rasterization -> softbuffer Win32 GDI presentation.
use crate::{renderer::Renderer, ui::Organizer};
use anyhow::{Context, Result};
use std::{
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

#[derive(Clone, Copy)]
struct Wake(Duration, u64);

#[derive(Default)]
pub struct RenderGate {
    pub dirty: bool,
    pub drawable: bool,
}
impl RenderGate {
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }
    pub fn take_frame(&mut self) -> bool {
        if !self.drawable || !self.dirty {
            return false;
        }
        self.dirty = false;
        true
    }
}

struct Host {
    ctx: egui::Context,
    app: Organizer,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    input: Option<egui_winit::State>,
    renderer: Renderer,
    gate: RenderGate,
    deadline: Option<Instant>,
    error: Option<anyhow::Error>,
    metrics: Metrics,
    occluded: bool,
}

#[derive(Default, serde::Serialize)]
struct Metrics {
    frames: u64,
    worker_or_repaint_wakes: u64,
    hidden_redraws_skipped: u64,
    layout_us: u128,
    raster_us: u128,
    present_us: u128,
    max_frame_us: u128,
}

pub fn run(directory: PathBuf, render_log: Option<PathBuf>) -> Result<()> {
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    let ctx = egui::Context::default();
    let proxy = event_loop.create_proxy();
    ctx.set_request_repaint_callback(move |info| {
        let _ = proxy.send_event(Wake(info.delay, info.current_cumulative_pass_nr));
    });
    let app = Organizer::new(&ctx, directory, true);
    let mut host = Host {
        ctx,
        app,
        window: None,
        surface: None,
        input: None,
        renderer: Renderer::new(),
        gate: RenderGate::default(),
        deadline: None,
        error: None,
        metrics: Metrics::default(),
        occluded: false,
    };
    event_loop.run_app(&mut host)?;
    if let Some(path) = render_log {
        file_organizer::filesystem::check_path(&path)?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        serde_json::to_writer_pretty(
            file,
            &serde_json::json!({"backend": "CPU rasterizer + Win32 GDI softbuffer", "metrics": host.metrics}),
        )?;
    }
    if let Some(error) = host.error {
        return Err(error);
    }
    Ok(())
}

impl Host {
    fn refresh_visibility(&mut self) {
        if let Some(w) = &self.window {
            let s = w.inner_size();
            self.gate.drawable = self.error.is_none()
                && !self.occluded
                && w.is_visible() != Some(false)
                && w.is_minimized() != Some(true)
                && s.width > 0
                && s.height > 0;
        }
    }
    fn frame(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        self.refresh_visibility();
        if !self.gate.drawable {
            self.metrics.hidden_redraws_skipped += 1;
        }
        if !self.gate.take_frame() {
            return Ok(());
        }
        let frame_start = Instant::now();
        let window = self.window.as_ref().context("Missing window")?;
        let input = self.input.as_mut().context("Missing input integration")?;
        let info = input
            .egui_input_mut()
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default();
        egui_winit::update_viewport_info(info, &self.ctx, window, false);
        let raw = input.take_egui_input(window);
        let output = self.ctx.run_ui(raw, |ui| self.app.paint(ui));
        self.metrics.layout_us += frame_start.elapsed().as_micros();
        input.handle_platform_output_with_event_loop(
            window,
            event_loop,
            output.platform_output.clone(),
        );
        let viewport = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .context("Missing root viewport")?;
        self.deadline = Instant::now().checked_add(viewport.repaint_delay);
        let mut actions = Vec::new();
        egui_winit::process_viewport_commands(
            &self.ctx,
            input
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default(),
            viewport.commands.clone(),
            window,
            &mut actions,
        );
        let size = window.inner_size();
        let raster_start = Instant::now();
        let pixels = self
            .renderer
            .draw(&self.ctx, output, size.width, size.height)?;
        self.metrics.raster_us += raster_start.elapsed().as_micros();
        let present_start = Instant::now();
        let surface = self.surface.as_mut().context("Missing software surface")?;
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        for (out, [r, g, b, _]) in buffer.iter_mut().zip(pixels) {
            *out = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
        }
        buffer
            .present()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        self.metrics.present_us += present_start.elapsed().as_micros();
        self.metrics.frames += 1;
        self.metrics.max_frame_us = self
            .metrics
            .max_frame_us
            .max(frame_start.elapsed().as_micros());
        Ok(())
    }
    fn poll(&mut self, event_loop: &ActiveEventLoop) {
        self.app.poll(&self.ctx);
        if self.app.can_close() {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<Wake> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            let icon = crate::branding::window_icon()?;
            let attributes = Window::default_attributes()
                .with_title("FileOrganizer")
                .with_window_icon(Some(icon.clone()))
                .with_inner_size(winit::dpi::LogicalSize::new(1160.0, 820.0))
                .with_min_inner_size(winit::dpi::LogicalSize::new(960.0, 680.0));
            #[cfg(windows)]
            let attributes = {
                use winit::platform::windows::WindowAttributesExtWindows;
                attributes.with_taskbar_icon(Some(icon))
            };
            let window = Arc::new(event_loop.create_window(attributes)?);
            let context = softbuffer::Context::new(window.clone())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            self.surface = Some(
                softbuffer::Surface::new(&context, window.clone())
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?,
            );
            self.input = Some(egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                window.as_ref(),
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(8192),
            ));
            self.app.set_dialog_parent(window.clone());
            self.window = Some(window);
            self.gate.invalidate();
            self.refresh_visibility();
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, Wake(delay, pass): Wake) {
        self.metrics.worker_or_repaint_wakes += 1;
        self.poll(event_loop);
        if pass.saturating_add(1) < self.ctx.cumulative_pass_nr() {
            return;
        }
        if delay.is_zero() {
            self.gate.invalidate();
        } else if let Some(time) = Instant::now().checked_add(delay) {
            self.deadline = Some(self.deadline.map_or(time, |d| d.min(time)));
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        self.poll(event_loop);
        if matches!(event, WindowEvent::CloseRequested) {
            self.app.close_requested();
            self.gate.invalidate();
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            // OS expose events may require presenting again; hidden/minimized events cannot paint.
            self.gate.invalidate();
            if let Err(error) = self.frame(event_loop) {
                self.error = Some(error);
                self.gate.drawable = false;
                self.deadline = None;
                self.app.close_requested();
            }
            return;
        }
        if let (Some(window), Some(input)) = (&self.window, &mut self.input)
            && input.on_window_event(window, &event).repaint
        {
            self.gate.invalidate();
        }
        if let WindowEvent::Occluded(occluded) = event {
            self.occluded = occluded;
            self.refresh_visibility();
        } else {
            self.refresh_visibility();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll(event_loop);
        if self.deadline.is_some_and(|d| d <= Instant::now()) {
            self.deadline = None;
            self.gate.invalidate();
        }
        if self.gate.drawable
            && self.gate.dirty
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(if self.gate.drawable {
            self.deadline
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil)
        } else {
            ControlFlow::Wait
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_updates_remain_dirty_without_rendering_and_restore_once() {
        let mut gate = RenderGate::default();
        for _ in 0..4 {
            gate.invalidate();
            assert!(!gate.take_frame());
        }
        gate.drawable = true;
        assert!(gate.take_frame());
        assert!(!gate.take_frame());
    }
}
