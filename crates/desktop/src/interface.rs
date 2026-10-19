use super::*;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 112, 245);
const INK: egui::Color32 = egui::Color32::from_rgb(30, 30, 30);
const MUTED: egui::Color32 = egui::Color32::from_rgb(113, 116, 123);
const BORDER: egui::Color32 = egui::Color32::from_rgb(224, 225, 230);
#[derive(Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Overview,
    Display,
    Sharing,
    Applications,
    Details,
    Recovery,
}
#[derive(Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Hardware,
    Graphics,
    Files,
    Devices,
}
fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(16)
}
fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(egui::Color32::WHITE)
            .strong(),
    )
    .fill(ACCENT)
    .min_size(egui::vec2(110.0, 36.0))
}
fn icon(ui: &mut egui::Ui, os: GuestOs, size: f32) {
    interface_assets::image(
        ui,
        match os {
            GuestOs::Windows => "windows",
            GuestOs::Ubuntu => "ubuntu",
        },
        size,
    );
}
pub fn install_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "FreeLane UI".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/DejaVuSans.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "FreeLane UI".into());
    fonts.font_data.insert(
        "FreeLane semibold".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/DejaVuSans-Bold.ttf"
        ))),
    );
    let mut semibold_family = fonts.families[&egui::FontFamily::Proportional].clone();
    semibold_family.insert(0, "FreeLane semibold".into());
    fonts.families.insert(
        egui::FontFamily::Name("FreeLane Semibold".into()),
        semibold_family,
    );
    #[cfg(target_os = "macos")]
    if let Ok(data) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        let mut semibold = egui::FontData::from_owned(data.clone());
        if semibold
            .variation_axes()
            .iter()
            .any(|axis| axis.tag.to_string() == "wght")
        {
            semibold.tweak.coords.push(b"wght", 600.0);
            fonts
                .font_data
                .insert("macOS semibold".into(), std::sync::Arc::new(semibold));
            fonts
                .families
                .get_mut(&egui::FontFamily::Name("FreeLane Semibold".into()))
                .unwrap()
                .insert(0, "macOS semibold".into());
        }
        fonts.font_data.insert(
            "macOS system".into(),
            std::sync::Arc::new(egui::FontData::from_owned(data)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "macOS system".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Light);
    ctx.global_style_mut(|style| {
        style.visuals.panel_fill = egui::Color32::from_rgb(255, 255, 255);
        style.visuals.window_fill = egui::Color32::WHITE;
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::WHITE;
        style.visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
        style.visuals.window_corner_radius = egui::CornerRadius::same(12);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(243, 244, 246);
        style.visuals.override_text_color = Some(INK);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(218, 233, 255);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, INK);
        style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(239, 240, 243);
        style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(226, 231, 239);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(214, 230, 252);
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style.spacing.button_padding = egui::vec2(13.0, 8.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
    });
}
impl Desktop {
    pub fn draw(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language;
        egui::Panel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(10),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("FreeLane")
                            .size(15.0)
                            .strong()
                            .color(INK),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("language")
                            .selected_text(if self.language == Language::Vietnamese {
                                "Tiếng Việt"
                            } else {
                                "English"
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.language,
                                    Language::Vietnamese,
                                    "Tiếng Việt",
                                );
                                ui.selectable_value(
                                    &mut self.language,
                                    Language::English,
                                    "English",
                                );
                            });
                    });
                });
            });
        egui::Panel::left("library")
            .default_size(300.0)
            .size_range(210.0..=340.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(244, 245, 248))
                    .inner_margin(16),
            )
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                self.library_ui(ui, ctx);
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(255, 255, 255))
                    .inner_margin(28),
            )
            .show(ui, |ui| {
                let viewport_height = ui.available_height();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.view == View::Recovery {
                        self.recovery_ui(ui, ctx);
                    } else if self.vm.is_some() {
                        self.machine_ui(ui, ctx, viewport_height);
                    } else {
                        ui.vertical_centered(|ui| {
                            ui.add_space(75.0);
                            ui.horizontal(|ui| {
                                let offset = ((ui.available_width() - 150.0) / 2.0).max(0.0);
                                ui.add_space(offset);
                                icon(ui, GuestOs::Windows, 64.0);
                                icon(ui, GuestOs::Ubuntu, 64.0);
                            });
                            ui.add_space(24.0);
                            ui.heading(lang.text("Your next workspace starts here"));
                            ui.label(lang.text("Open a Windows or Ubuntu configuration to begin."));
                            ui.add_space(18.0);
                            if ui.add(primary(lang.text("Open virtual machine"))).clicked() {
                                self.open_dialog = true;
                            }
                            if ui.button(lang.text("Restore virtual machine")).clicked() {
                                self.view = View::Recovery;
                            }
                        });
                    }
                    if self.pending.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                        });
                    }
                    if !self.library_notice.is_empty() {
                        ui.colored_label(
                            MUTED,
                            format!(
                                "{}: {}",
                                lang.text("Library not saved"),
                                self.library_notice
                            ),
                        );
                    }
                    if !self.notice.is_empty() {
                        ui.add_space(10.0);
                        ui.collapsing(lang.text("Notifications"), |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(100.0)
                                .show(ui, |ui| {
                                    ui.label(lang.message(&self.notice));
                                });
                        });
                    }
                });
            });
        if self.open_dialog {
            let mut open = true;
            egui::Window::new(lang.text("Open virtual machine"))
                .id(egui::Id::new("open-dialog"))
                .open(&mut open)
                .default_width(550.0)
                .default_pos(ctx.content_rect().center() - egui::vec2(275.0, 100.0))
                .show(ctx, |ui| {
                    ui.label(lang.text("Configuration file"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft_path)
                            .desired_width(f32::INFINITY)
                            .hint_text("/path/to/vm.toml"),
                    );
                    if ui
                        .add_enabled(
                            self.pending.is_none()
                                && self.child.is_none()
                                && !self.draft_path.trim().is_empty(),
                            primary(lang.text("Open")),
                        )
                        .clicked()
                    {
                        self.open(ctx);
                    }
                });
            self.open_dialog = open;
        }
        self.settings_ui(ctx);
    }
    fn machine_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, viewport_height: f32) {
        let lang = self.language;
        let vm = self.vm.as_ref().unwrap();
        let (name, os, cpus, memory, agent_enabled, path, acceleration) = (
            vm.name.clone(),
            vm.guest_os,
            vm.cpus,
            vm.memory,
            vm.guest_agent,
            vm.path.clone(),
            vm.acceleration,
        );
        let busy = self.pending.is_some();
        let compact = ui.available_width() < 650.0;
        ui.horizontal(|ui| {
            icon(ui, os, if compact { 44.0 } else { 64.0 });
            ui.add_space(8.0);
            let actions = if compact { 0.0 } else { 310.0 };
            let title_width = (ui.available_width() - actions).max(90.0);
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, 65.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width(title_width);
                    ui.add(
                        egui::Label::new(
                            interface_assets::semibold(&name)
                                .size(if compact { 23.0 } else { 32.0 })
                                .strong(),
                        )
                        .truncate(),
                    );
                    ui.horizontal(|ui| {
                        ui.painter().circle_filled(
                            ui.cursor().min + egui::vec2(5.0, 8.0),
                            4.0,
                            if self.status == "running" {
                                egui::Color32::from_rgb(35, 154, 100)
                            } else {
                                MUTED
                            },
                        );
                        ui.add_space(16.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(lang.text(&self.status)).color(MUTED),
                            )
                            .truncate(),
                        );
                    });
                },
            );
            if !compact {
                self.machine_actions(ui, ctx, false);
            }
        });
        if compact {
            ui.horizontal_wrapped(|ui| {
                self.machine_actions(ui, ctx, true);
            });
        }
        ui.add_space(18.0);
        if matches!(self.view, View::Overview | View::Display) {
            self.preview_ui(ui, ctx, viewport_height);
            ui.add_space(18.0);
        }
        let column = ((ui.available_width() - 48.0) / 3.0).max(90.0);
        ui.horizontal(|ui| {
            for (symbol, value, label) in [
                ("cpu", cpus.to_string(), "Virtual CPUs"),
                (
                    "display",
                    format!("{} GiB", memory as f64 / 1024.0),
                    "Memory",
                ),
                ("cpu", "ARM64".to_owned(), "Architecture"),
            ] {
                ui.allocate_ui(egui::vec2(column, 48.0), |ui| {
                    ui.set_min_width(column);
                    ui.horizontal(|ui| {
                        interface_assets::image(ui, symbol, 26.0);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.label(interface_assets::semibold(value).size(17.0));
                            ui.colored_label(MUTED, lang.text(label));
                        });
                    });
                });
            }
        });
        ui.add_space(18.0);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.view, View::Overview, lang.text("Overview"));
            ui.selectable_value(&mut self.view, View::Sharing, lang.text("Sharing"));
            ui.selectable_value(
                &mut self.view,
                View::Applications,
                lang.text("Applications"),
            );
            ui.selectable_value(&mut self.view, View::Display, lang.text("Display"));
            ui.selectable_value(&mut self.view, View::Details, lang.text("Details"));
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(16.0);
        match self.view {
            View::Overview => {
                ui.columns(if compact { 1 } else { 2 }, |columns| {
                    egui::Grid::new("vm-overview")
                        .spacing(egui::vec2(24.0, 12.0))
                        .show(&mut columns[0], |ui| {
                            for (label, value) in [
                                (
                                    "Operating system",
                                    match os {
                                        GuestOs::Windows => "Windows",
                                        GuestOs::Ubuntu => "Ubuntu",
                                    }
                                    .to_owned(),
                                ),
                                ("Architecture", "ARM64".to_owned()),
                                ("Virtual CPUs", cpus.to_string()),
                                ("Memory", format!("{} GiB", memory as f64 / 1024.0)),
                            ] {
                                ui.colored_label(MUTED, lang.text(label));
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                    let index = columns.len() - 1;
                    if compact {
                        columns[index].add_space(20.0);
                    }
                    self.shared_folder_row(&mut columns[index], ctx);
                });
            }
            View::Sharing => {
                self.shared_folder_row(ui, ctx);
                ui.add_space(16.0);
                self.clipboard_ui(ui, ctx);
                self.transfer_ui(ui, ctx);
            }
            View::Display => {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            !busy,
                            interface_assets::button(ctx, "refresh", lang.text("Refresh display")),
                        )
                        .on_hover_text(lang.text("Snapshots, not live video"))
                        .clicked()
                    {
                        self.capture(ctx, true);
                    }
                    if ui
                        .checkbox(&mut self.auto_display, lang.text("Refresh every 5 seconds"))
                        .changed()
                    {
                        self.display_at = None;
                    }
                });
            }
            View::Applications => {
                card().show(ui, |ui| {
                    ui.heading(lang.text("Guest connection"));
                    if ui
                        .add_enabled(
                            agent_enabled && !busy,
                            egui::Button::new(lang.text("Check connection")),
                        )
                        .clicked()
                    {
                        self.guest_request(ctx, false);
                    }
                    if ui
                        .add_enabled(
                            agent_enabled && !busy,
                            egui::Button::new(lang.text("Find guest applications")),
                        )
                        .clicked()
                    {
                        self.discover_apps(ctx);
                    }
                    if !self.app_catalog.is_empty() {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.app_catalog_search)
                                .hint_text(lang.text("Search applications")),
                        );
                        let search = self.app_catalog_search.to_lowercase();
                        egui::ScrollArea::vertical()
                            .id_salt("app-catalog")
                            .max_height(150.0)
                            .show(ui, |ui| {
                                for app in &self.app_catalog {
                                    if !(search.is_empty()
                                        || app.name.to_lowercase().contains(&search))
                                    {
                                        continue;
                                    }
                                    if ui
                                        .selectable_label(
                                            self.app_path == app.executable,
                                            &app.name,
                                        )
                                        .on_hover_text(&app.executable)
                                        .clicked()
                                    {
                                        self.app_path = app.executable.clone();
                                        self.app_name = app.name.clone();
                                        self.app_arguments.clear();
                                    }
                                }
                            });
                    }
                    ui.label(lang.text("Executable path"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.app_path)
                            .desired_width(f32::INFINITY)
                            .hint_text(if os == GuestOs::Ubuntu {
                                "/absolute/path/to/app"
                            } else {
                                r"C:\Windows\System32\notepad.exe"
                            }),
                    );
                    ui.label(lang.text("Arguments (one per line)"));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.app_arguments)
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    if ui
                        .add_enabled(
                            agent_enabled && !busy && !self.app_path.trim().is_empty(),
                            primary(lang.text("Launch app")),
                        )
                        .clicked()
                    {
                        self.guest_request(ctx, true);
                    }
                    if os == GuestOs::Windows {
                        ui.separator();
                        ui.heading(lang.text("Independent app window & Dock"));
                        ui.label(lang.text("App name"));
                        ui.add(egui::TextEdit::singleline(&mut self.app_name).hint_text("Notepad"));
                        ui.label(lang.text("New app profile path"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.app_profile_output)
                                .desired_width(f32::INFINITY)
                                .hint_text("/absolute/path/to/notepad.app.toml"),
                        );
                        let ready = agent_enabled
                            && !busy
                            && !self.app_name.trim().is_empty()
                            && !self.app_profile_output.trim().is_empty()
                            && !self.app_path.trim().is_empty();
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(lang.text("Save app profile")),
                                )
                                .clicked()
                            {
                                self.save_app_profile(ctx, false);
                            }
                            if ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(lang.text("Save & open separate window")),
                                )
                                .clicked()
                            {
                                self.save_app_profile(ctx, true);
                            }
                        });
                        if ui
                            .add_enabled(
                                !busy && !self.app_profile_output.trim().is_empty(),
                                egui::Button::new(lang.text("Open saved app profile")),
                            )
                            .clicked()
                        {
                            self.open_saved_app(ctx);
                        }
                        ui.label(lang.text("New Dock .app path (Mac only)"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.dock_output)
                                .desired_width(f32::INFINITY)
                                .hint_text("/Users/you/Applications/Windows Notepad.app"),
                        );
                        if ui
                            .add_enabled(
                                cfg!(target_os = "macos")
                                    && !busy
                                    && !self.app_profile_output.trim().is_empty()
                                    && !self.dock_output.trim().is_empty(),
                                egui::Button::new(lang.text("Export Dock launcher")),
                            )
                            .clicked()
                        {
                            self.export_app_dock(ctx);
                        }
                    }
                    if !agent_enabled {
                        ui.small(
                            lang.text("Install and run the guest agent to launch applications."),
                        );
                    }
                });
            }
            View::Recovery => self.recovery_ui(ui, ctx),
            View::Details => {
                card().show(ui, |ui| {
                    if ui
                        .add_enabled(!busy, egui::Button::new(lang.text("Refresh resources")))
                        .clicked()
                    {
                        let profile = path.clone();
                        self.work(ctx, move || {
                            Ok(match freelane::telemetry::vm(&profile) {
                                Ok(value) => Event::Resources(profile, value),
                                Err(error) => Event::Notice(error.to_string()),
                            })
                        });
                    }
                    if let Some(value) = &self.resources {
                        if let Some(bytes) = value["resources"]["resident_bytes"].as_u64() {
                            ui.label(format!(
                                "{}: {:.1} MiB",
                                lang.text("Resident memory"),
                                bytes as f64 / 1048576.0
                            ));
                        }
                        if let Some(nanos) = value["resources"]["cpu_nanoseconds"].as_u64() {
                            ui.label(format!(
                                "{}: {:.2} s",
                                lang.text("CPU time"),
                                nanos as f64 / 1e9
                            ));
                        }
                    }
                    ui.label(lang.text("Configuration path"));
                    ui.label(path.display().to_string());
                    ui.label(lang.text("Guest agent"));
                    ui.label(lang.text(&self.agent_status));
                    if ui
                        .add_enabled(
                            agent_enabled && !busy,
                            egui::Button::new(lang.text("Probe capabilities")),
                        )
                        .clicked()
                    {
                        self.probe_agent_capabilities(ctx);
                    }
                    if let Some(info) = &self.agent_info {
                        ui.label(format!(
                            "{}: v{}",
                            lang.text("Protocol"),
                            info.negotiated_protocol
                        ));
                        if let Some(version) = &info.agent_version {
                            ui.label(format!("{}: {}", lang.text("Agent version"), version));
                        }
                        for (label, name, protocol) in [
                            ("Launch applications", "launch", 1),
                            ("Window snapshots", "window_snapshots", 2),
                        ] {
                            let state = match info.supports(name, protocol) {
                                Some(true) => "Supported",
                                Some(false) => "Unavailable",
                                None => "Unknown",
                            };
                            ui.label(format!("{}: {}", lang.text(label), lang.text(state)));
                        }
                    }
                    ui.label(lang.text("Configured acceleration"));
                    ui.label(lang.text(acceleration));
                    ui.collapsing(lang.text("Advanced controls"), |ui| {
                        ui.small(lang.text("Force stop discards RAM and can lose unsaved data."));
                        if ui
                            .add_enabled(!busy, egui::Button::new(lang.text("Force stop")))
                            .clicked()
                        {
                            self.control(ctx, "stop");
                        }
                    });
                });
            }
        }
    }
    pub fn settings_ui(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let lang = self.language;
        let mut open = true;
        let mut save = false;
        let mut import = false;
        let mut export = false;
        let mut pick_share = false;
        let mut inspect_share = false;
        egui::Window::new(lang.text("Virtual machine settings")).id(egui::Id::new("settings"))
            .open(&mut open).default_pos(ctx.content_rect().center() - egui::vec2(305.0, 240.0)).default_width(610.0).default_height(480.0).show(ctx, |ui| {
            ui.small(lang.text("Changes apply on the next start. Shut down before saving."));
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.settings_tab, SettingsTab::Hardware, lang.text("Hardware"));
                ui.selectable_value(&mut self.settings_tab, SettingsTab::Graphics, lang.text("Graphics"));
                ui.selectable_value(&mut self.settings_tab, SettingsTab::Files, lang.text("Files"));
                ui.selectable_value(&mut self.settings_tab, SettingsTab::Devices, lang.text("Devices"));
            }); ui.separator();
            egui::ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                let Some(config) = self.settings.as_mut() else { return };
                match self.settings_tab {
                    SettingsTab::Hardware => {
                        super::resource_ui::presets(ui,config,self.resource_capacity,lang);
                        ui.separator();
                        ui.label(lang.text("Name")); ui.text_edit_singleline(&mut config.name);
                        ui.add(egui::Slider::new(&mut config.cpus, 1..=64).text(lang.text("Virtual CPUs")));
                        ui.horizontal(|ui| { ui.label(lang.text("RAM (MiB)")); ui.add(egui::DragValue::new(&mut config.memory_mib).range(2048..=1_048_576).speed(256)); });
                        if let Some(runtime) = config.runtime.as_mut() {
                            ui.label(lang.text("Acceleration")); ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut runtime.accelerator,qemu::Accelerator::Hvf,lang.text("Mac hardware · HVF"));
                                ui.selectable_value(&mut runtime.accelerator,qemu::Accelerator::Tcg,lang.text("Software · TCG"));
                            });
                            if runtime.accelerator == qemu::Accelerator::Hvf { runtime.tcg_threading = None; }
                            else { ui.horizontal(|ui| {
                                ui.selectable_value(&mut runtime.tcg_threading,None,lang.text("Default"));
                                ui.selectable_value(&mut runtime.tcg_threading,Some(qemu::TcgThreading::Single),lang.text("Single"));
                                ui.selectable_value(&mut runtime.tcg_threading,Some(qemu::TcgThreading::Multi),lang.text("Multi"));
                            }); }
                        }
                    }
                    SettingsTab::Graphics => {
                        if let Some(runtime) = config.runtime.as_mut() {
                            ui.selectable_value(&mut runtime.graphics,qemu::Graphics::Ramfb,lang.text("Basic display"));
                            ui.add_enabled_ui(config.guest_os == GuestOs::Ubuntu, |ui| { ui.selectable_value(&mut runtime.graphics,qemu::Graphics::Virtio2d,lang.text("Virtio GPU · 2D")).on_hover_text(lang.text("Virtio 2D requires an Ubuntu driver. It does not enable Metal, 3D or Windows GPU acceleration.")); });
                            ui.checkbox(&mut runtime.headless,lang.text("Headless · hide native VM window"));
                        }
                    }
                    SettingsTab::Files => {
                        ui.label(format!("{}: {}",lang.text("Guest disk"),config.disk.display()));
                        ui.label(lang.text("Installer / CD image (blank to detach)")); ui.add(egui::TextEdit::singleline(&mut self.installer_path).desired_width(f32::INFINITY));
                        ui.label(lang.text("Host folder (blank to disable)")); if ui.add(egui::TextEdit::singleline(&mut self.shared_path).desired_width(f32::INFINITY)).changed(){self.shared_report=None;}
                        ui.horizontal(|ui|{if ui.add_enabled(self.pending.is_none(),egui::Button::new(lang.text("Choose shared folder"))).clicked(){pick_share=true;}
                        if ui.add_enabled(self.pending.is_none(),egui::Button::new(lang.text("Check folder"))).clicked(){inspect_share=true;}});
                        if let Some(report)=&self.shared_report {use freelane::sharing::HostState;let label=match report.host {HostState::Disabled=>"Sharing disabled",HostState::Available=>"Host folder available",HostState::Missing=>"Folder missing · reconnect and choose again",HostState::NotDirectory=>"Selected path is not a folder",HostState::VolumeUnavailable=>"Drive unavailable · reconnect and choose again",HostState::Unavailable=>"Cannot inspect shared folder"};ui.label(lang.text(label)); if report.guest_mount=="mounted" {ui.label(lang.text("Guest FreeLane mount observed · host source unverified"));for mount in &report.guest_mounts{ui.label(format!("{} · {}",mount.path,if mount.readonly{lang.text("Read-only")}else{lang.text("Read/write")}));}} else if report.guest_mount=="not_mounted" {ui.label(lang.text("FreeLane share is not mounted in guest"));}}
                        if let Some(runtime) = config.runtime.as_mut() {
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut runtime.share_mode,qemu::ShareMode::ReadonlyUsb,lang.text("Read-only USB"));
                                ui.add_enabled_ui(config.guest_os == GuestOs::Ubuntu, |ui| { ui.selectable_value(&mut runtime.share_mode,qemu::ShareMode::Virtio9p,lang.text("Live read/write · Ubuntu")); ui.selectable_value(&mut runtime.share_mode,qemu::ShareMode::Virtio9pReadonly,lang.text("Live read-only · Ubuntu")); });
                            });
                            ui.small(lang.text(if runtime.share_mode == qemu::ShareMode::ReadonlyUsb { "Keep FAT shares under 504 MiB and unchanged while the VM runs." } else if runtime.share_mode==qemu::ShareMode::Virtio9pReadonly {"Live read-only share; mount it inside Ubuntu."} else { "Guest writes modify host files. Mount the share inside Ubuntu; see docs/sharing.md." }));
                        }
                    }
                    SettingsTab::Devices => {
                        if let Some(runtime) = config.runtime.as_mut() {
                            ui.checkbox(&mut runtime.network,lang.text("NAT networking"));
                            if runtime.network { ui.horizontal_wrapped(|ui| {
                                ui.label(lang.text("Network adapter"));
                                ui.selectable_value(&mut runtime.network_device,qemu::NetworkDevice::Virtio,"Virtio PCI");
                                ui.selectable_value(&mut runtime.network_device,qemu::NetworkDevice::Usb,lang.text("USB network"));
                            }); }
                            ui.checkbox(&mut runtime.guest_agent,lang.text("Guest agent channel"));
                            ui.checkbox(&mut runtime.serial_console,lang.text("Private serial console"));
                            ui.separator(); ui.label(lang.text("Physical USB devices")).on_hover_text(lang.text("Inspect IDs with scripts/list-usb.py. Device capture needs OS access; addresses may change."));
                            let mut remove = None;
                            for (index,device) in runtime.usb_devices.iter_mut().enumerate() { ui.horizontal_wrapped(|ui| {
                                ui.label("Bus"); ui.add(egui::DragValue::new(&mut device.bus).range(1..=255));
                                ui.label(lang.text("Address")); ui.add(egui::DragValue::new(&mut device.address).range(1..=127));
                                ui.label("VID"); ui.add(egui::DragValue::new(&mut device.vendor_id).hexadecimal(4,false,true));
                                ui.label("PID"); ui.add(egui::DragValue::new(&mut device.product_id).hexadecimal(4,false,true));
                                if ui.button(lang.text("Remove")).clicked() { remove = Some(index); }
                            }); }
                            if let Some(index) = remove { runtime.usb_devices.remove(index); }
                            if ui.add_enabled(runtime.usb_devices.len()<16,egui::Button::new(lang.text("Add USB device"))).clicked() { runtime.usb_devices.push(qemu::UsbHostDevice{bus:1,address:1,vendor_id:0,product_id:0}); }
                        }
                    }
                }
            }); ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.pending.is_none() && self.child.is_none(),primary(lang.text("Save changes"))).clicked() { save = true; }
                if ui.add_enabled(self.pending.is_none(),egui::Button::new(lang.text("Import settings"))).clicked(){import=true;}
                if ui.add_enabled(self.pending.is_none(),egui::Button::new(lang.text("Export saved settings"))).clicked(){export=true;}
            });
            if !self.notice.is_empty() { ui.label(lang.message(&self.notice)); }
        });
        self.settings_open = open;
        if pick_share {
            self.pick_shared_folder(ctx);
        } else if inspect_share {
            self.inspect_shared_folder(ctx);
        } else if save {
            self.save_settings(ctx);
        } else if import {
            self.import_settings_ui(ctx);
        } else if export {
            self.export_settings_ui(ctx);
        }
    }
}

#[cfg(test)]
mod recovery_access_tests {
    use super::*;
    fn text_for(view: View) -> String {
        rendered(Desktop {
            language: Language::English,
            view,
            ..Default::default()
        })
    }
    fn rendered(mut desktop: Desktop) -> String {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 820.0),
                )),
                ..Default::default()
            },
            |ui| desktop.draw(ui, &ctx),
        );
        fn collect(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(value) => {
                    text.push_str(value.galley.text());
                    text.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, text)
                    }
                }
                _ => {}
            }
        }
        output.textures_delta.clear();
        let mut text = String::new();
        for shape in output.shapes {
            collect(&shape.shape, &mut text);
        }
        text
    }
    #[test]
    fn library_search_matches_name_case_and_guest_type() {
        let desktop = Desktop {
            language: Language::English,
            library_search: "  UBUNTU  ".into(),
            library: vec![
                (
                    "/missing-windows.toml".into(),
                    "Accounting fixture".into(),
                    GuestOs::Windows,
                ),
                (
                    "/missing-ubuntu.toml".into(),
                    "Build fixture".into(),
                    GuestOs::Ubuntu,
                ),
            ],
            ..Default::default()
        };
        let text = rendered(desktop);
        assert!(text.contains("Build fixture"));
        assert!(!text.contains("Accounting fixture"));
    }
    #[test]
    fn vietnamese_library_search_is_case_insensitive() {
        let desktop = Desktop {
            language: Language::Vietnamese,
            library_search: "MÁY".into(),
            library: vec![
                (
                    "/missing-a.toml".into(),
                    "Máy học tập".into(),
                    GuestOs::Ubuntu,
                ),
                (
                    "/missing-b.toml".into(),
                    "Accounting fixture".into(),
                    GuestOs::Windows,
                ),
            ],
            ..Default::default()
        };
        let text = rendered(desktop);
        assert!(text.contains("Máy học tập"));
        assert!(!text.contains("Accounting fixture"));
    }
    #[test]
    fn empty_library_exposes_restore_without_an_existing_vm() {
        assert!(text_for(View::Display).contains("Restore virtual machine"));
    }
    #[test]
    fn recovery_screen_can_choose_a_checkpoint_without_loading_a_vm() {
        assert!(text_for(View::Recovery).contains("Choose checkpoint"));
    }
}
