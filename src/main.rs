#![windows_subsystem = "windows"]

pub mod audio;
pub mod config;
pub mod gui;
pub mod hotkey;
pub mod tray;
pub mod startup;
pub mod utils;
pub mod com_interfaces;

use eframe::egui;

fn main() -> eframe::Result<()> {
    println!("Starting MicMuteRs...");
    
    // Load config
    let app_config = config::AppConfig::load();

    let audio_ctrl = match audio::AudioController::new(app_config.device_id.as_ref()) {
        Ok(ac) => ac,
        Err(e) => {
            eprintln!("Failed to initialize audio controller with configured device ID: {}. Attempting with default device.", e);
            audio::AudioController::new(None).expect("Failed to initialize default audio controller")
        }
    };
    
    let mut initial_hotkeys = Vec::new();
    let get_vk = |val: &serde_json::Value| -> u32 {
        val.get("vk").and_then(|v| v.as_u64()).unwrap_or(0) as u32
    };
    if let Some(h) = app_config.hotkey.get("toggle") { 
        let vk = get_vk(h); if vk != 0 { initial_hotkeys.push(vk); } 
    }
    if let Some(h) = app_config.hotkey.get("mute") { 
        let vk = get_vk(h); if vk != 0 { initial_hotkeys.push(vk); } 
    }
    if let Some(h) = app_config.hotkey.get("unmute") { 
        let vk = get_vk(h); if vk != 0 { initial_hotkeys.push(vk); } 
    }
    if initial_hotkeys.is_empty() {
        initial_hotkeys.push(0xB3); // Play/Pause default
    }

    let hotkey_manager = hotkey::HotkeyManager::new(initial_hotkeys);

    let devices = audio::get_audio_devices().unwrap_or_default();

    // Initialize tray icon
    let tray = tray::TrayManager::new(&app_config, &devices);

    // Start UI
    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_visible(true)
            .with_decorations(false)
            .with_taskbar(false)
            .with_active(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([if app_config.persistent_overlay.show_vu { 40.0 } else { 26.0 }, 26.0])
            .with_position([app_config.persistent_overlay.x as f32, app_config.persistent_overlay.y as f32])
            .with_title("MicMuteRs Overlay"),
        depth_buffer: 0,
        multisampling: 0,
        ..Default::default()
    };

    eframe::run_native(
        "MicMuteRs",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(gui::MicMuteApp::new(cc, audio_ctrl, hotkey_manager, tray, app_config, devices)))
        }),
    )
}
