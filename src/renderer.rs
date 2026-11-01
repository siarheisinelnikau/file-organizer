//! CPU renderer shared by the desktop host and finite offscreen captures.
//! The cached rasterizer consumes egui 0.33 meshes; this explicit adapter preserves
//! egui 0.37 geometry, premultiplied colors, texture updates and clipping exactly.
use anyhow::{Result, bail, ensure};
use egui_raster as raster;
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};

pub struct Renderer {
    backend: EguiSoftwareRender,
}
impl Renderer {
    pub fn new() -> Self {
        Self {
            backend: EguiSoftwareRender::new(ColorFieldOrder::Rgba),
        }
    }

    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        mut output: egui::FullOutput,
        width: u32,
        height: u32,
    ) -> Result<Vec<[u8; 4]>> {
        ensure!(
            width >= 64 && height >= 64 && u64::from(width) * u64::from(height) <= 16_000_000,
            "Viewport exceeds the CPU rendering limit"
        );
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let mut meshes = Vec::with_capacity(primitives.len());
        for primitive in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                bail!("GPU paint callbacks are unsupported by the CPU renderer")
            };
            meshes.push(raster::ClippedPrimitive {
                clip_rect: rect(primitive.clip_rect),
                primitive: raster::epaint::Primitive::Mesh(raster::Mesh {
                    indices: mesh.indices,
                    vertices: mesh
                        .vertices
                        .into_iter()
                        .map(|v| raster::epaint::Vertex {
                            pos: pos(v.pos),
                            uv: pos(v.uv),
                            color: color(v.color),
                        })
                        .collect(),
                    texture_id: texture_id(mesh.texture_id),
                }),
            });
        }
        let delta = raster::TexturesDelta {
            set: output
                .textures_delta
                .set
                .drain()
                .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
                .map(|(id, delta)| {
                    let egui::ImageData::Color(image) = delta.image;
                    let pixels = image.pixels.iter().copied().map(color).collect();
                    (
                        texture_id(id),
                        raster::epaint::ImageDelta {
                            image: raster::ImageData::Color(std::sync::Arc::new(
                                raster::ColorImage::new(image.size, pixels),
                            )),
                            options: raster::TextureOptions {
                                magnification: filter(delta.options.magnification),
                                minification: filter(delta.options.minification),
                                wrap_mode: match delta.options.wrap_mode {
                                    egui::TextureWrapMode::ClampToEdge => {
                                        raster::TextureWrapMode::ClampToEdge
                                    }
                                    egui::TextureWrapMode::Repeat => {
                                        raster::TextureWrapMode::Repeat
                                    }
                                    egui::TextureWrapMode::MirroredRepeat => {
                                        raster::TextureWrapMode::MirroredRepeat
                                    }
                                },
                                mipmap_mode: delta.options.mipmap_mode.map(filter),
                            },
                            pos: delta.pos,
                        },
                    )
                })
                .collect(),
            free: output.textures_delta.free.drain().map(texture_id).collect(),
        };
        let background = ctx.global_style().visuals.panel_fill.to_array();
        let mut pixels = vec![background; width as usize * height as usize];
        self.backend.render(
            &mut BufferMutRef::new(&mut pixels, width as usize, height as usize),
            &meshes,
            &delta,
            output.pixels_per_point,
        );
        Ok(pixels)
    }
}
fn texture_id(id: egui::TextureId) -> raster::TextureId {
    match id {
        egui::TextureId::Managed(id) => raster::TextureId::Managed(id),
        egui::TextureId::User(id) => raster::TextureId::User(id),
    }
}
fn color(c: egui::Color32) -> raster::Color32 {
    let [r, g, b, a] = c.to_array();
    raster::Color32::from_rgba_premultiplied(r, g, b, a)
}
fn pos(p: egui::Pos2) -> raster::Pos2 {
    raster::pos2(p.x, p.y)
}
fn rect(r: egui::Rect) -> raster::Rect {
    raster::Rect::from_min_max(pos(r.min), pos(r.max))
}
fn filter(f: egui::TextureFilter) -> raster::TextureFilter {
    match f {
        egui::TextureFilter::Nearest => raster::TextureFilter::Nearest,
        egui::TextureFilter::Linear => raster::TextureFilter::Linear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_preserves_ordered_partial_texture_updates_and_clip_coordinates() {
        let ctx = egui::Context::default();
        let mut mesh = egui::Mesh::with_texture(egui::TextureId::Managed(42));
        let bounds = egui::Rect::from_min_max(egui::pos2(16.0, 16.0), egui::pos2(112.0, 112.0));
        mesh.add_rect_with_uv(
            bounds,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(128.0, 128.0),
                )),
                ..Default::default()
            },
            |_| {},
        );
        output.shapes.clear();
        output.shapes.push(egui::epaint::ClippedShape {
            clip_rect: bounds,
            shape: egui::Shape::mesh(mesh),
            transform_after_tessellation: Default::default(),
        });
        let id = egui::TextureId::Managed(42);
        output.textures_delta.push(
            id,
            egui::epaint::ImageDelta::full(
                egui::ColorImage::filled([2, 2], egui::Color32::RED),
                egui::TextureOptions::NEAREST,
            ),
        );
        output.textures_delta.push(
            id,
            egui::epaint::ImageDelta::partial(
                [0, 0],
                egui::ColorImage::filled([1, 1], egui::Color32::BLUE),
                egui::TextureOptions::NEAREST,
            ),
        );
        output.textures_delta.push(
            id,
            egui::epaint::ImageDelta::partial(
                [1, 1],
                egui::ColorImage::filled([1, 1], egui::Color32::GREEN),
                egui::TextureOptions::NEAREST,
            ),
        );
        let pixels = Renderer::new().draw(&ctx, output, 128, 128).unwrap();
        assert_eq!(pixels[32 * 128 + 32], [0, 0, 255, 255]);
        assert_eq!(pixels[96 * 128 + 96], [0, 255, 0, 255]);
        assert_eq!(pixels[0], ctx.global_style().visuals.panel_fill.to_array());
    }

    #[test]
    fn cpu_canvas_uses_the_selected_theme_instead_of_a_fixed_light_background() {
        let ctx = egui::Context::default();
        crate::design::configure(&ctx);
        let mut colors = Vec::new();
        for theme in [
            file_organizer::localization::Theme::Light,
            file_organizer::localization::Theme::Dark,
        ] {
            crate::design::apply_theme(&ctx, theme);
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.shapes.clear();
            let pixels = Renderer::new().draw(&ctx, output, 64, 64).unwrap();
            assert!(
                pixels
                    .iter()
                    .all(|pixel| *pixel == ctx.global_style().visuals.panel_fill.to_array())
            );
            colors.push(pixels[0]);
        }
        assert_ne!(colors[0], colors[1]);
    }
}
