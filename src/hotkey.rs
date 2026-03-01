use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN, KBDLLHOOKSTRUCT
};

static HOTKEY_SENDER: OnceLock<Sender<u32>> = OnceLock::new();
static RECORDING_MODE: AtomicBool = AtomicBool::new(false);
static RECORD_SENDER: OnceLock<Sender<u32>> = OnceLock::new();

static TARGET_VKS: [AtomicU32; 3] = [
    AtomicU32::new(0),
    AtomicU32::new(0),
    AtomicU32::new(0),
];

pub struct HotkeyManager {
    receiver: Receiver<u32>,
    record_receiver: Receiver<u32>,
}

impl HotkeyManager {
    pub fn new(vks: Vec<u32>) -> Self {
        let (sender, receiver) = channel();
        let (rec_sender, record_receiver) = channel();
        
        let _ = HOTKEY_SENDER.set(sender);
        let _ = RECORD_SENDER.set(rec_sender);
        
        for (i, &vk) in vks.iter().take(3).enumerate() {
            TARGET_VKS[i].store(vk, Ordering::SeqCst);
        }

        thread::spawn(|| {
            unsafe {
                let hook = SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(hook_callback),
                    None,
                    0,
                ).expect("Failed to install keyboard hook");

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    // Message loop is required for the hook to receive events
                }

                let _ = UnhookWindowsHookEx(hook);
            }
        });

        Self { receiver, record_receiver }
    }

    pub fn set_hotkeys(&self, vks: Vec<u32>) {
        for i in 0..3 {
            let val = if i < vks.len() { vks[i] } else { 0 };
            TARGET_VKS[i].store(val, Ordering::SeqCst);
        }
    }
    
    
    pub fn try_recv(&self) -> Option<u32> {
        self.receiver.try_recv().ok()
    }

    pub fn start_recording(&self) {
        RECORDING_MODE.store(true, Ordering::SeqCst);
    }

    pub fn try_recv_record(&self) -> Option<u32> {
        if let Ok(vk) = self.record_receiver.try_recv() {
            RECORDING_MODE.store(false, Ordering::SeqCst);
            Some(vk)
        } else {
            None
        }
    }
}

unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let w_param_u32 = w_param.0 as u32;
        if w_param_u32 == WM_KEYDOWN || w_param_u32 == WM_SYSKEYDOWN {
            let kbd_struct = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
            
            if RECORDING_MODE.load(Ordering::SeqCst) {
                if let Some(sender) = RECORD_SENDER.get() {
                    let _ = sender.send(kbd_struct.vkCode);
                    // Consume the keypress so it doesn't do anything else while recording
                    return windows::Win32::Foundation::LRESULT(1);
                }
            } else {
                for target_atomic in &TARGET_VKS {
                    let target = target_atomic.load(Ordering::SeqCst);
                    if target != 0 && kbd_struct.vkCode == target {
                        if let Some(sender) = HOTKEY_SENDER.get() {
                            let _ = sender.send(kbd_struct.vkCode);
                            return windows::Win32::Foundation::LRESULT(1);
                        }
                    }
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}
