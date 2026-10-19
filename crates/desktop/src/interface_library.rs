use super::*;

impl Desktop {
    pub(super) fn library_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let lang = self.language;
        let can_switch = self.pending.is_none() && self.child.is_none() && !self.settings_open;
        ui.horizontal(|ui| {
            interface_assets::image(ui, "search", 16.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.library_search)
                    .frame(egui::Frame::NONE)
                    .desired_width((ui.available_width() - 30.0).max(80.0))
                    .hint_text(lang.text("Find virtual machines")),
            );
            if !self.library_search.is_empty()
                && ui
                    .add(interface_assets::button(ctx, "close", "").frame(false))
                    .on_hover_text(lang.text("Clear search"))
                    .clicked()
            {
                self.library_search.clear();
            }
        });
        ui.add_space(16.0);
        let query = self.library_search.trim().to_lowercase();
        let mut chosen = None;
        let mut repair = None;
        egui::ScrollArea::vertical()
            .id_salt("library-scroll")
            .max_height((ui.available_height() - 65.0).max(80.0))
            .show(ui, |ui| {
                let mut count = 0;
                for (path, name, os) in &self.library {
                    let guest = match os {
                        GuestOs::Windows => "windows",
                        GuestOs::Ubuntu => "ubuntu",
                    };
                    if !query.is_empty()
                        && !name.to_lowercase().contains(&query)
                        && !guest.contains(&query)
                    {
                        continue;
                    }
                    count += 1;
                    let selected = self.vm.as_ref().is_some_and(|vm| vm.path == *path);
                    let response = egui::Frame::new()
                        .fill(if selected {
                            egui::Color32::from_rgb(218, 233, 253)
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .corner_radius(9)
                        .inner_margin(12)
                        .show(ui, |ui| {
                            ui.set_min_width((ui.available_width() - 1.0).max(100.0));
                            ui.horizontal(|ui| {
                                interface_assets::image(ui, guest, 34.0);
                                ui.add_space(3.0);
                                ui.vertical(|ui| {
                                    ui.add(
                                        egui::Label::new(interface_assets::semibold(name))
                                            .truncate(),
                                    );
                                    let subtitle = if selected {
                                        lang.text(&self.status).to_owned()
                                    } else {
                                        format!(
                                            "{} · ARM64",
                                            match os {
                                                GuestOs::Windows => "Windows",
                                                GuestOs::Ubuntu => "Ubuntu",
                                            }
                                        )
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(subtitle)
                                                .small()
                                                .color(egui::Color32::from_rgb(106, 112, 123)),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                        })
                        .response;
                    let response = ui.interact(
                        response.rect,
                        response.id.with("select"),
                        egui::Sense::click(),
                    );
                    if response.clicked() && can_switch {
                        chosen = Some(path.clone());
                    }
                    response
                        .on_hover_text(path.display().to_string())
                        .context_menu(|ui| {
                            if ui
                                .add_enabled(can_switch, egui::Button::new(lang.text("Locate VM")))
                                .clicked()
                            {
                                repair = Some(path.clone());
                                ui.close();
                            }
                        });
                    ui.add_space(4.0);
                }
                if count == 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(113, 116, 123),
                        lang.text(if query.is_empty() {
                            "No virtual machines"
                        } else {
                            "No results"
                        }),
                    );
                }
            });
        ui.add_space((ui.available_height() - 42.0).max(12.0));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.pending.is_none() && self.child.is_none(),
                    interface_assets::button(ctx, "plus", lang.text("New VM")).frame(false),
                )
                .clicked()
            {
                self.wizard.begin();
            }
            ui.menu_button(
                egui::Image::new((
                    interface_assets::texture(ctx, "dots").id(),
                    egui::vec2(18.0, 18.0),
                )),
                |ui| {
                    if ui
                        .add_enabled(
                            can_switch,
                            egui::Button::new(lang.text("Add virtual machine")),
                        )
                        .clicked()
                    {
                        self.open_dialog = true;
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.pending.is_none(),
                            egui::Button::new(lang.text("Restore virtual machine")),
                        )
                        .clicked()
                    {
                        self.view = interface::View::Recovery;
                        ui.close();
                    }
                    if ui.button(lang.text("Android & iOS")).clicked() {
                        self.mobile_open = true;
                        ui.close();
                    }
                },
            );
        });
        if let Some(path) = repair {
            self.repair_bookmark(ctx, path);
        }
        if let Some(path) = chosen {
            self.draft_path = path.display().to_string();
            self.open(ctx);
        }
    }
}
