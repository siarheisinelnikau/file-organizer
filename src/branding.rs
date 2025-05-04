//! One checked-in image supplies both native window and taskbar icons.
pub fn window_icon() -> anyhow::Result<winit::window::Icon> {
    let image = image::load_from_memory(include_bytes!("../assets/icon-256.png"))?.into_rgba8();
    let (width, height) = image.dimensions();
    Ok(winit::window::Icon::from_rgba(
        image.into_raw(),
        width,
        height,
    )?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_icon_decodes_without_creating_a_window() {
        super::window_icon().expect("The embedded RGBA icon must be valid");
    }
}
