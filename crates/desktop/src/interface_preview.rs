use super::*;

impl Desktop {
    pub(super) fn machine_actions(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        _compact: bool,
    ) {
        let lang = self.language;
        let busy = self.pending.is_some();
        if ui
            .add_enabled(
                !busy,
                interface_assets::button(ctx, "settings", lang.text("Settings")),
            )
            .clicked()
        {
            self.notice.clear();
            self.resource_capacity = freelane::resource_presets::HostCapacity::detect();
            self.settings_open = true;
        }
        let can_start =
            !busy && self.child.is_none() && !matches!(self.status.as_str(), "running" | "paused");
        let play = interface_assets::texture(ctx, "play");
        if ui
            .add_enabled(
                can_start,
                egui::Button::image_and_text(
                    egui::Image::new((play.id(), egui::vec2(16.0, 16.0)))
                        .tint(egui::Color32::WHITE),
                    egui::RichText::new(lang.text("Start"))
                        .color(egui::Color32::WHITE)
                        .strong(),
                )
                .fill(egui::Color32::from_rgb(0, 112, 245))
                .min_size(egui::vec2(110.0, 36.0)),
            )
            .clicked()
        {
            self.start();
        }
        ui.menu_button(
            egui::Image::new((
                interface_assets::texture(ctx, "dots").id(),
                egui::vec2(18.0, 18.0),
            )),
            |ui| {
                for (label, action, allowed) in [
                    ("Pause", "pause", self.status == "running"),
                    ("Resume", "resume", self.status == "paused"),
                    (
                        "Shutdown",
                        "shutdown",
                        matches!(self.status.as_str(), "running" | "paused"),
                    ),
                    ("Refresh status", "status", true),
                ] {
                    if allowed
                        && ui
                            .add_enabled(!busy, egui::Button::new(lang.text(label)))
                            .clicked()
                    {
                        self.control(ctx, action);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(lang.text("Backups")).clicked() {
                    self.view = interface::View::Recovery;
                    ui.close();
                }
            },
        );
    }

    pub(super) fn preview_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        viewport_height: f32,
    ) {
        let width = ui.available_width();
        let height = (width / 2.27).min((viewport_height * 0.48).max(180.0));
        if let Some(texture) = &self.texture {
            // Input coordinates are mapped to the actual displayed snapshot, never the decoration.
            let frame = ui.add(
                egui::Image::new(texture)
                    .max_size(egui::vec2(width, height))
                    .corner_radius(14)
                    .sense(egui::Sense::click()),
            );
            if frame.clicked() && self.pending.is_none() && self.status == "running" {
                if let Some(point) = frame.interact_pointer_pos() {
                    let x = ((point.x - frame.rect.left()) / frame.rect.width() * 32767.0)
                        .clamp(0.0, 32767.0)
                        .round() as u16;
                    let y = ((point.y - frame.rect.top()) / frame.rect.height() * 32767.0)
                        .clamp(0.0, 32767.0)
                        .round() as u16;
                    self.click(ctx, x, y);
                }
            }
        } else {
            let texture = interface_assets::texture(ctx, "preview");
            let source_ratio = texture.size_vec2().x / texture.size_vec2().y;
            let visible_height = (source_ratio / (width / height)).min(1.0);
            let uv = egui::Rect::from_min_max(
                egui::pos2(0.0, (1.0 - visible_height) / 2.0),
                egui::pos2(1.0, (1.0 + visible_height) / 2.0),
            );
            let frame = ui.add(
                egui::Image::new(&texture)
                    .uv(uv)
                    .fit_to_exact_size(egui::vec2(width, height))
                    .maintain_aspect_ratio(false)
                    .corner_radius(14),
            );
            let badge = egui::Rect::from_center_size(frame.rect.center(), egui::vec2(132.0, 132.0));
            let mut overlay = ui.new_child(egui::UiBuilder::new().max_rect(badge));
            {
                let ui = &mut overlay;
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(145))
                    .corner_radius(66)
                    .inner_margin(14)
                    .show(ui, |ui| {
                        ui.set_width(104.0);
                        ui.set_min_height(104.0);
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            let symbol = interface_assets::texture(ctx, "power");
                            ui.add(
                                egui::Image::new(&symbol)
                                    .fit_to_exact_size(egui::vec2(36.0, 36.0))
                                    .tint(egui::Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new(self.language.text(&self.status))
                                    .size(13.0)
                                    .color(egui::Color32::WHITE),
                            );
                        });
                    });
            }
        }
    }

    pub(super) fn shared_folder_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language;
        ui.label(egui::RichText::new(lang.text("Shared folder")).strong());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            interface_assets::image(ui, "folder", 32.0);
            ui.vertical(|ui| {
                let name = Path::new(&self.shared_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(lang.text("Not configured"));
                ui.add(egui::Label::new(egui::RichText::new(name).strong()).truncate());
                if !self.shared_path.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.shared_path)
                                .color(egui::Color32::from_rgb(113, 116, 123)),
                        )
                        .truncate(),
                    );
                }
            });
        });
        ui.add_space(8.0);
        if ui
            .add_enabled(
                self.pending.is_none(),
                interface_assets::button(ctx, "settings", lang.text("Configure sharing"))
                    .frame(false),
            )
            .clicked()
        {
            self.settings_tab = interface::SettingsTab::Files;
            self.resource_capacity = freelane::resource_presets::HostCapacity::detect();
            self.settings_open = true;
        }
    }
}
