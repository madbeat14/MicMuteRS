use eframe::egui;
use crate::config::AppConfig;
use std::time::{Instant, Duration};
use std::sync::mpsc;
use std::thread;

pub struct MicMuteApp {
    show_settings: bool,
    audio: crate::audio::AudioController,
    hotkeys: crate::hotkey::HotkeyManager,
    tray: crate::tray::TrayManager,
    config: AppConfig,
    is_muted: bool,
    peak_level: f32,
    osd_timer: Option<Instant>,
    startup_enabled: bool,
    is_light_theme: bool,
    last_theme_check: Instant,
    recording_key: Option<String>,
    available_devices: Vec<(String, String)>,
    last_debug_event: String,
    tray_rx: mpsc::Receiver<String>,
}

impl MicMuteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        audio: crate::audio::AudioController,
        hotkeys: crate::hotkey::HotkeyManager,
        mut tray: crate::tray::TrayManager,
        config: AppConfig,
        available_devices: Vec<(String, String)>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx_clone = cc.egui_ctx.clone();
        
        // Spawn Background Tray Poller
        thread::spawn(move || {
            loop {
                // Poll Menu Events
                if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                    let id = event.id.0.as_str();
                    if id == "quit" {
                        std::process::exit(0);
                    }
                    let _ = tx.send(id.to_string());
                    ctx_clone.request_repaint(); // Wake UI thread
                }
                
                // Poll Tray Icon Events (Double Click)
                if let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
                    if let tray_icon::TrayIconEvent::DoubleClick { button: tray_icon::MouseButton::Left, .. } = event {
                        let _ = tx.send("toggle_mute".to_string());
                        ctx_clone.request_repaint(); // Wake UI thread
                    }
                }
                
                thread::sleep(Duration::from_millis(16)); // ~60fps polling
            }
        });
        
        let is_muted = audio.is_muted().unwrap_or(false);
        let is_light_theme = crate::utils::is_system_light_theme();
        tray.set_icon_state(is_muted, is_light_theme);
        Self {
            show_settings: false,
            audio,
            hotkeys,
            tray,
            config,
            is_muted,
            peak_level: 0.0,
            osd_timer: None,
            startup_enabled: crate::startup::get_run_on_startup(),
            is_light_theme,
            last_theme_check: Instant::now(),
            recording_key: None,
            available_devices,
            last_debug_event: "Initialized".to_string(),
            tray_rx: rx,
        }
    }

    fn trigger_osd(&mut self) {
        if self.config.osd.enabled {
            self.osd_timer = Some(Instant::now());
        }
    }
}

impl eframe::App for MicMuteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(vk) = self.hotkeys.try_recv_record() {
            if let Some(key_name) = &self.recording_key {
                if let Some(cfg) = self.config.hotkey.get_mut(key_name) {
                    if let Some(obj) = cfg.as_object_mut() {
                        obj.insert("vk".to_string(), serde_json::json!(vk));
                        obj.insert("name".to_string(), serde_json::json!(crate::utils::vk_to_string(vk)));
                    }
                    self.config.save();
                    
                    let mut vks = Vec::new();
                    let get_vk = |val: &serde_json::Value| -> u32 {
                        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                    };
                    if let Some(h) = self.config.hotkey.get("toggle") { 
                        let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                    }
                    if let Some(h) = self.config.hotkey.get("mute") { 
                        let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                    }
                    if let Some(h) = self.config.hotkey.get("unmute") { 
                        let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                    }
                    self.hotkeys.set_hotkeys(vks);
                }
            }
            self.recording_key = None;
        }

        // Poll Background Tray Commands
        while let Ok(id) = self.tray_rx.try_recv() {
            match id.as_str() {
                "toggle_mute" => {
                    match self.audio.toggle_mute(&self.config) {
                        Ok((muted, debug)) => {
                            self.is_muted = muted;
                            self.last_debug_event = debug;
                            self.tray.set_icon_state(self.is_muted, self.is_light_theme);
                            self.trigger_osd();
                        }
                        Err(e) => {
                            eprintln!("[ERROR] toggle_mute failed: {:?}", e);
                        }
                    }
                }
                "toggle_sound" => {
                    self.config.beep_enabled = !self.config.beep_enabled;
                    self.config.save();
                    self.tray.update_menu(&self.config, &self.available_devices);
                }
                "toggle_osd" => {
                    self.config.osd.enabled = !self.config.osd.enabled;
                    self.config.save();
                    self.tray.update_menu(&self.config, &self.available_devices);
                }
                "toggle_overlay" => {
                    self.config.persistent_overlay.enabled = !self.config.persistent_overlay.enabled;
                    self.config.save();
                    self.tray.update_menu(&self.config, &self.available_devices);
                }
                "toggle_boot" => {
                    self.startup_enabled = !self.startup_enabled;
                    crate::startup::set_run_on_startup(self.startup_enabled);
                    self.tray.update_menu(&self.config, &self.available_devices);
                }
                "settings" => {
                    self.show_settings = !self.show_settings;
                }
                "help" => {
                    let _ = open::that("https://github.com/papopon/MicMuteRs");
                }
                "about" => {
                    self.last_debug_event = "MicMuteRs v0.1.0".to_string();
                }
                id if id.starts_with("mic_") => {
                    let dev_id = &id[4..];
                    self.config.device_id = Some(dev_id.to_string());
                    self.config.save();
                    if let Ok(ac) = crate::audio::AudioController::new(self.config.device_id.as_ref()) {
                        self.audio = ac;
                        self.is_muted = self.audio.is_muted().unwrap_or(false);
                        self.tray.set_icon_state(self.is_muted, self.is_light_theme);
                    }
                    self.tray.update_menu(&self.config, &self.available_devices);
                }
                _ => {}
            }
        }

        // Remove old polling logic
        /*
        if let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if let tray_icon::TrayIconEvent::DoubleClick { button: tray_icon::MouseButton::Left, .. } = event {
                // ...
            }
        }
        */

        if let Some(vk) = self.hotkeys.try_recv() {
            let mut action = None;
            for (key, cfg) in &self.config.hotkey {
                let cfg_vk = cfg.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if cfg_vk == vk {
                    action = Some(key.as_str());
                    break;
                }
            }
            
            match action {
                Some("toggle") => {
                    match self.audio.toggle_mute(&self.config) {
                        Ok((muted, debug)) => {
                            self.is_muted = muted;
                            self.last_debug_event = debug;
                            self.tray.set_icon_state(self.is_muted, self.is_light_theme);
                            self.trigger_osd();
                        }
                        Err(e) => {
                            eprintln!("[ERROR] Hotkey toggle_mute failed: {:?}", e);
                        }
                    }
                }
                Some("mute") => {
                    if !self.is_muted {
                        if let Ok(debug) = self.audio.set_mute(true, &self.config) {
                            self.last_debug_event = debug;
                        }
                        self.is_muted = true;
                        self.audio.play_feedback(true, &self.config);
                        self.tray.set_icon_state(true, self.is_light_theme);
                        self.trigger_osd();
                    }
                }
                Some("unmute") => {
                    if self.is_muted {
                        if let Ok(debug) = self.audio.set_mute(false, &self.config) {
                            self.last_debug_event = debug;
                        }
                        self.is_muted = false;
                        self.audio.play_feedback(false, &self.config);
                        self.tray.set_icon_state(false, self.is_light_theme);
                        self.trigger_osd();
                    }
                }
                _ => {}
            }
        }

        // Theme check loop (Throttle to 5s)
        if self.last_theme_check.elapsed().as_secs() >= 5 {
            self.last_theme_check = Instant::now();
            let current_theme = crate::utils::is_system_light_theme();
            if current_theme != self.is_light_theme {
                self.is_light_theme = current_theme;
                self.tray.set_icon_state(self.is_muted, self.is_light_theme);
            }
        }

        // AFK Auto-mute check
        if self.config.afk.enabled && !self.is_muted {
            let idle = crate::utils::get_idle_duration();
            if idle > self.config.afk.timeout as f32 {
                if let Ok((muted, debug)) = self.audio.toggle_mute(&self.config) {
                    self.is_muted = muted;
                    self.last_debug_event = debug;
                    self.tray.set_icon_state(self.is_muted, self.is_light_theme);
                    self.trigger_osd();
                }
            }
        }

        // Sync External Mute State & Update Peak Level
        let ext_muted = self.audio.is_muted().unwrap_or(self.is_muted);
        if ext_muted != self.is_muted {
            self.is_muted = ext_muted;
            self.tray.set_icon_state(self.is_muted, self.is_light_theme);
            // Optionally we could trigger OSD here, but external changes might spam it.
        }

        self.peak_level = self.audio.get_peak_value().unwrap_or(0.0);

        // Root Viewport acts as the Overlay
        if self.config.persistent_overlay.enabled {
            let overlay_width = if self.config.persistent_overlay.show_vu { 40.0 } else { 26.0 };
            
            // Maintain root window bounds for the overlay
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(overlay_width, 26.0)));
            
            // Read current window position back to config to avoid snap-back when locking
            if !self.config.persistent_overlay.locked {
                if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                    self.config.persistent_overlay.x = rect.min.x as i32;
                    self.config.persistent_overlay.y = rect.min.y as i32;
                }
            }

            // Lock position if enabled, else allow dragging
            if self.config.persistent_overlay.locked {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    egui::Pos2::new(self.config.persistent_overlay.x as f32, self.config.persistent_overlay.y as f32)
                ));
                // Architectural Fix: True OS Mouse Passthrough
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            }

            let response = egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let opacity_tint = egui::Color32::from_white_alpha((self.config.persistent_overlay.opacity as f32 / 100.0 * 255.0) as u8);
                        if self.is_muted {
                            let img = if self.is_light_theme {
                                egui::include_image!("../assets/mic_muted_black.svg")
                            } else {
                                egui::include_image!("../assets/mic_muted_white.svg")
                            };
                            ui.add(egui::Image::new(img).max_height(24.0).tint(opacity_tint));
                        } else {
                            let img = if self.is_light_theme {
                                egui::include_image!("../assets/mic_black.svg")
                            } else {
                                egui::include_image!("../assets/mic_white.svg")
                            };
                            ui.add(egui::Image::new(img).max_height(24.0).tint(opacity_tint));
                            
                            // VU Meter
                            if self.config.persistent_overlay.show_vu {
                                let (rect, _response) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                                let threshold = self.config.persistent_overlay.sensitivity as f32 / 100.0;
                                let color = if self.peak_level > threshold {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(rect, 5.0, color);
                            }
                        }
                    });
                }).response;

            if !self.config.persistent_overlay.locked && response.interact(egui::Sense::click_and_drag()).dragged() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                // We'd ideally save the new position here, but egui's window positions are tricky to read back continuously without an event.
                // We'll rely on the user dragging it.
            }
        } else {
            // Hide the root window if overlay is disabled
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT)).show(ctx, |ui| { });
        }

        // Settings Viewport
        if self.show_settings {
            let settings_id = egui::ViewportId::from_hash_of("settings_menu_v2");
            let settings_builder = egui::ViewportBuilder::default()
                .with_title("MicMuteRs Settings")
                .with_inner_size([400.0, 750.0])
                .with_min_inner_size([300.0, 300.0])
                .with_resizable(true)
                .with_taskbar(false)
                .with_active(true);

            ctx.show_viewport_immediate(settings_id, settings_builder, |ctx, _class| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.show_settings = false;
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("MicMuteRs Settings");
                
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.show_settings, true, "General");
                    // Simple text-based tab simulation since we don't have an enum yet, we'll just show them sequentially with CollapsingHeaders for now
                });
                ui.separator();

                let mut config_changed = false;

                egui::CollapsingHeader::new("Audio Device Selection").default_open(true).show(ui, |ui| {
                    let mut selected_id = self.config.device_id.clone();
                    
                    egui::ComboBox::from_label("Microphone")
                        .width(300.0)
                        .selected_text(
                            if selected_id.is_none() {
                                "Default Windows Device".to_string()
                            } else {
                                self.available_devices.iter()
                                    .find(|(id, _)| Some(id) == selected_id.as_ref())
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_else(|| "Unknown (Not Connected)".to_string())
                            }
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_value(&mut selected_id, None, "Default Windows Device").changed() {
                                config_changed = true;
                                self.config.device_id = None;
                                if let Ok(new_audio) = crate::audio::AudioController::new(None) {
                                    self.audio = new_audio;
                                }
                            }
                            
                            for (id, name) in &self.available_devices {
                                if ui.selectable_value(&mut selected_id, Some(id.clone()), name).changed() {
                                    config_changed = true;
                                    self.config.device_id = Some(id.clone());
                                    if let Ok(new_audio) = crate::audio::AudioController::new(Some(id)) {
                                        self.audio = new_audio;
                                    }
                                }
                            }
                        });
                        
                    if ui.button("Refresh Devices").clicked() {
                        self.available_devices = crate::audio::get_audio_devices().unwrap_or_default();
                    }
                });

                egui::CollapsingHeader::new("Sync Devices (Simultaneous Mute)").default_open(true).show(ui, |ui| {
                    ui.label("Devices selected here will mute/unmute alongside the primary device.");
                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        for (id, name) in &self.available_devices {
                            if Some(id.clone()) == self.config.device_id {
                                continue;
                            }
                            let mut is_synced = self.config.sync_ids.contains(id);
                            if ui.checkbox(&mut is_synced, name).changed() {
                                config_changed = true;
                                if is_synced {
                                    self.config.sync_ids.push(id.clone());
                                } else {
                                    self.config.sync_ids.retain(|x| x != id);
                                }
                            }
                        }
                    });
                });

                egui::CollapsingHeader::new("General Audio & Feedback").default_open(true).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(format!("State: {}", if self.is_muted { "Muted" } else { "Active" }));
                        if ui.button("Toggle Mute").clicked() {
                            if let Ok((muted, debug)) = self.audio.toggle_mute(&self.config) {
                                self.is_muted = muted;
                                self.last_debug_event = debug;
                                self.tray.set_icon_state(self.is_muted, self.is_light_theme);
                                self.trigger_osd();
                            }
                        }
                    });
                    ui.label(format!("GUI Mute Target Debug: {}", self.last_debug_event));
                    ui.label(format!("Peak Level (VU): {:.2}", self.peak_level));
                    ui.separator();
                    config_changed |= ui.checkbox(&mut self.config.beep_enabled, "Enable Audio Feedback (Beep/WAV)").changed();
                    ui.horizontal(|ui| {
                        ui.label("Feedback Mode:");
                        config_changed |= ui.radio_value(&mut self.config.audio_mode, "beep".to_string(), "Beep").changed();
                        config_changed |= ui.radio_value(&mut self.config.audio_mode, "custom".to_string(), "Custom WAV").changed();
                    });
                });

                egui::CollapsingHeader::new("System & Startup").default_open(true).show(ui, |ui| {
                    if ui.checkbox(&mut self.startup_enabled, "Start MicMute on Boot").changed() {
                        crate::startup::set_run_on_startup(self.startup_enabled);
                    }
                    ui.separator();
                    config_changed |= ui.checkbox(&mut self.config.afk.enabled, "Enable AFK Auto-Mute").changed();
                    ui.add_enabled_ui(self.config.afk.enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Timeout (seconds):");
                            config_changed |= ui.add(egui::Slider::new(&mut self.config.afk.timeout, 5..=3600)).changed();
                        });
                    });
                });

                egui::CollapsingHeader::new("Hotkeys (VK Codes)").default_open(true).show(ui, |ui| {
                    let mut update_hotkeys = false;
                    let get_vk = |val: &serde_json::Value| -> u32 {
                        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                    };
                    for key in ["toggle", "mute", "unmute"] {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} VK Code:", key));
                            let current_vk = self.config.hotkey.get(key).map(|c| get_vk(c)).unwrap_or(0);
                            ui.label(format!("{}", current_vk));
                            
                            let is_recording = self.recording_key.as_deref() == Some(key);
                            let btn_text = if is_recording { "Press a key..." } else { "Record" };
                            
                            if ui.button(btn_text).clicked() && !is_recording {
                                self.recording_key = Some(key.to_string());
                                self.hotkeys.start_recording();
                            }
                            
                            if ui.button("Clear").clicked() {
                                if let Some(cfg_mut) = self.config.hotkey.get_mut(key) {
                                    if let Some(obj) = cfg_mut.as_object_mut() {
                                        obj.insert("vk".to_string(), serde_json::json!(0));
                                    }
                                    update_hotkeys = true;
                                    config_changed = true;
                                }
                            }
                        });
                    }
                    if update_hotkeys {
                        let mut vks = Vec::new();
                        if let Some(h) = self.config.hotkey.get("toggle") { 
                            let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                        }
                        if let Some(h) = self.config.hotkey.get("mute") { 
                            let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                        }
                        if let Some(h) = self.config.hotkey.get("unmute") { 
                            let vk = get_vk(h); if vk != 0 { vks.push(vk); } 
                        }
                        self.hotkeys.set_hotkeys(vks);
                    }
                });

                egui::CollapsingHeader::new("Persistent Overlay").default_open(true).show(ui, |ui| {
                    config_changed |= ui.checkbox(&mut self.config.persistent_overlay.enabled, "Enable Persistent Overlay").changed();
                    let enabled = self.config.persistent_overlay.enabled;
                    ui.add_enabled_ui(enabled, |ui| {
                        config_changed |= ui.checkbox(&mut self.config.persistent_overlay.locked, "Lock Position (Disable Dragging)").changed();
                        config_changed |= ui.checkbox(&mut self.config.persistent_overlay.show_vu, "Show VU Meter").changed();
                        ui.horizontal(|ui| {
                            ui.label("Opacity:");
                            config_changed |= ui.add(egui::Slider::new(&mut self.config.persistent_overlay.opacity, 10..=100)).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.label("VU Sensitivity:");
                            config_changed |= ui.add(egui::Slider::new(&mut self.config.persistent_overlay.sensitivity, 1..=50)).changed();
                        });
                    });
                });

                egui::CollapsingHeader::new("OSD (On-Screen Display)").default_open(true).show(ui, |ui| {
                    config_changed |= ui.checkbox(&mut self.config.osd.enabled, "Enable OSD Notifications").changed();
                    let enabled = self.config.osd.enabled;
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Duration (ms):");
                            config_changed |= ui.add(egui::Slider::new(&mut self.config.osd.duration, 500..=5000)).changed();
                        });
                        ui.horizontal(|ui| {
                            ui.label("Size:");
                            config_changed |= ui.add(egui::Slider::new(&mut self.config.osd.size, 50..=300)).changed();
                        });
                    });
                });

                if config_changed {
                    self.config.save();
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                    if ui.button("Hide Settings").clicked() {
                        self.show_settings = false;
                    }
                });
            });
        });
        }
        
        // Render OSD Viewport
        if let Some(start_time) = self.osd_timer {
            if start_time.elapsed().as_millis() < self.config.osd.duration as u128 {
                let osd_id = egui::ViewportId::from_hash_of("osd_v2");
                let osd_builder = egui::ViewportBuilder::default()
                    .with_title("MicMute OSD V2")
                    .with_inner_size([self.config.osd.size as f32, self.config.osd.size as f32])
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_taskbar(false)
                    .with_always_on_top()
                    .with_mouse_passthrough(true);

                ctx.show_viewport_immediate(osd_id, osd_builder, |ctx, _class| {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(true));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT).inner_margin(10.0))
                        .show(ctx, |ui| {
                            ui.centered_and_justified(|ui| {
                                let img = if self.is_muted {
                                    if self.is_light_theme {
                                        egui::include_image!("../assets/mic_muted_black.svg")
                                    } else {
                                        egui::include_image!("../assets/mic_muted_white.svg")
                                    }
                                } else {
                                    if self.is_light_theme {
                                        egui::include_image!("../assets/mic_black.svg")
                                    } else {
                                        egui::include_image!("../assets/mic_white.svg")
                                    }
                                };
                                ui.add(egui::Image::new(img).max_height(self.config.osd.size as f32 * 0.5));
                            });
                        });
                });
            } else {
                self.osd_timer = None;
            }
        }

        ctx.request_repaint(); // Continuous repaint for VU meter and OSD animations
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}
