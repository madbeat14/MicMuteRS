use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu, CheckMenuItem, MenuId},
    TrayIconBuilder, TrayIcon, Icon,
};
use crate::config::AppConfig;

pub struct TrayManager {
    tray_icon: TrayIcon,
}

impl TrayManager {
    pub fn new(config: &AppConfig, devices: &[(String, String)]) -> Self {
        let menu = Self::create_menu(config, devices);
        let icon = Self::load_icon(false, crate::utils::is_system_light_theme());

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("MicMuteRs")
            .with_icon(icon)
            .build()
            .expect("Failed to create tray icon");

        Self {
            tray_icon,
        }
    }

    pub fn update_menu(&mut self, config: &AppConfig, devices: &[(String, String)]) {
        let menu = Self::create_menu(config, devices);
        let _ = self.tray_icon.set_menu(Some(Box::new(menu)));
    }

    fn create_menu(config: &AppConfig, devices: &[(String, String)]) -> Menu {
        let menu = Menu::new();
        
        // Core Toggle
        let _ = menu.append(&MenuItem::with_id(MenuId::new("toggle_mute"), "Toggle Mute", true, None));
        let _ = menu.append(&PredefinedMenuItem::separator());

        // Select Microphone Submenu
        let mic_submenu = Submenu::new("Select Microphone", true);
        for (id, name) in devices {
            let is_selected = config.device_id.as_ref() == Some(id);
            // Use a specific ID scheme: "mic_ID"
            let item = CheckMenuItem::with_id(
                MenuId::new(format!("mic_{}", id)),
                name,
                true,
                is_selected,
                None
            );
            let _ = mic_submenu.append(&item);
        }
        let _ = menu.append(&mic_submenu);

        // Toggles
        let play_sound_i = CheckMenuItem::with_id(MenuId::new("toggle_sound"), "Play Sound on Toggle", true, config.beep_enabled, None);
        let osd_i = CheckMenuItem::with_id(MenuId::new("toggle_osd"), "Enable OSD Notification", true, config.osd.enabled, None);
        let overlay_i = CheckMenuItem::with_id(MenuId::new("toggle_overlay"), "Show Persistent Overlay", true, config.persistent_overlay.enabled, None);
        let boot_i = CheckMenuItem::with_id(MenuId::new("toggle_boot"), "Start on Boot", true, false, None); // Placeholder for now

        let _ = menu.append_items(&[
            &play_sound_i,
            &osd_i,
            &overlay_i,
            &boot_i,
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(MenuId::new("settings"), "Settings", true, None),
            &MenuItem::with_id(MenuId::new("help"), "Help", true, None),
            &MenuItem::with_id(MenuId::new("about"), "About", true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(MenuId::new("quit"), "Exit", true, None),
        ]);

        menu
    }

    pub fn set_icon_state(&mut self, is_muted: bool, is_light_theme: bool) {
        let icon = Self::load_icon(is_muted, is_light_theme);
        let _ = self.tray_icon.set_icon(Some(icon));
    }

    fn load_icon(is_muted: bool, is_light_theme: bool) -> Icon {
        let pk_data = match (is_muted, is_light_theme) {
            (true, true) => include_bytes!("../assets/mic_muted_black.png").as_slice(),
            (false, true) => include_bytes!("../assets/mic_black.png").as_slice(),
            (true, false) => include_bytes!("../assets/mic_muted_white.png").as_slice(),
            (false, false) => include_bytes!("../assets/mic_white.png").as_slice(),
        };

        let img = image::load_from_memory(pk_data).expect("Failed to load PNG").into_rgba8();
        let width = img.width();
        let height = img.height();
        let rgba = img.into_raw();

        Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon")
    }
}
