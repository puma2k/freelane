//! Bundled raster assets, decoded once per egui context (never in the repaint loop).
use super::*;

pub fn texture(ctx: &egui::Context, name: &'static str) -> egui::TextureHandle {
    let id = egui::Id::new(("mac-minimal-asset", name));
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let bytes: &[u8] = match name {
        "windows" => include_bytes!("../assets/mac-minimal/windows.png"),
        "ubuntu" => include_bytes!("../assets/mac-minimal/ubuntu.png"),
        "search" => include_bytes!("../assets/mac-minimal/search.png"),
        "plus" => include_bytes!("../assets/mac-minimal/plus.png"),
        "dots" => include_bytes!("../assets/mac-minimal/dots.png"),
        "settings" => include_bytes!("../assets/mac-minimal/settings.png"),
        "play" => include_bytes!("../assets/mac-minimal/player-play-filled.png"),
        "power" => include_bytes!("../assets/mac-minimal/power.png"),
        "folder" => include_bytes!("../assets/mac-minimal/folder-filled.png"),
        "cpu" => include_bytes!("../assets/mac-minimal/cpu.png"),
        "display" => include_bytes!("../assets/mac-minimal/device-desktop.png"),
        "chevron" => include_bytes!("../assets/mac-minimal/chevron-right.png"),
        "close" => include_bytes!("../assets/mac-minimal/x.png"),
        "refresh" => include_bytes!("../assets/mac-minimal/refresh.png"),
        "preview" => include_bytes!("../assets/mac-minimal/preview.png"),
        _ => unreachable!("unknown bundled UI asset"),
    };
    // Every asset is embedded and validated by the desktop build/capture checks.
    let image = image::load_from_memory(bytes)
        .expect("valid bundled UI asset")
        .to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    );
    let texture = ctx.load_texture(name, color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

pub fn image(ui: &mut egui::Ui, name: &'static str, size: f32) {
    let texture = texture(ui.ctx(), name);
    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(size, size)));
}

pub fn button<'a>(ctx: &egui::Context, name: &'static str, label: &'a str) -> egui::Button<'a> {
    let texture = texture(ctx, name);
    egui::Button::image_and_text(
        egui::Image::new((texture.id(), egui::vec2(18.0, 18.0))),
        label,
    )
}

/// egui `strong()` changes color, not font weight; use an actual semibold face.
pub fn semibold(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(egui::FontFamily::Name("FreeLane Semibold".into()))
}
