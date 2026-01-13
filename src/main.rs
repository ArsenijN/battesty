#![windows_subsystem = "windows"]

mod battery;
mod icon;
mod settings;
mod ui;

use std::sync::{Arc, Mutex, OnceLock};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::PCWSTR;

use battery::BatteryMonitor;
use icon::create_battery_icon;
use ui::*;

const WM_TRAYICON: u32 = WM_USER + 1;
const ID_TRAY_ICON: u32 = 1;
const TIMER_UPDATE: usize = 1;
const TIMER_SAVE: usize = 2;

static MONITOR: OnceLock<Arc<Mutex<BatteryMonitor>>> = OnceLock::new();
static WM_TASKBARCREATED_MSG: OnceLock<u32> = OnceLock::new();

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let monitor = Arc::new(Mutex::new(BatteryMonitor::new()));
            let _ = MONITOR.set(monitor.clone());
            
            let taskbar_created = "TaskbarCreated\0".encode_utf16().collect::<Vec<u16>>();
            let msg_id = RegisterWindowMessageW(PCWSTR(taskbar_created.as_ptr()));
            let _ = WM_TASKBARCREATED_MSG.set(msg_id);
            
            add_tray_icon(hwnd, &monitor);
            
            // Immediate update on startup
            update_tray_icon(hwnd, &monitor);
            
            let interval = monitor.lock().unwrap().settings.update_interval_ms;
            let update_interval = if battery::DEBUG_MODE { 2000 } else { interval };
            SetTimer(hwnd, TIMER_UPDATE, update_interval, None);
            SetTimer(hwnd, TIMER_SAVE, 300000, None);
            
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            handle_power_event(wparam, hwnd);
            LRESULT(1)
        }
        WM_TIMER => {
            handle_timer_event(wparam, hwnd);
            LRESULT(0)
        }
        WM_TRAYICON => {
            handle_tray_event(lparam, hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            handle_menu_command(wparam, hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            cleanup_and_exit(hwnd);
            LRESULT(0)
        }
        _ => {
            if let Some(&taskbar_msg) = WM_TASKBARCREATED_MSG.get() {
                if msg == taskbar_msg && msg != 0 {
                    if let Some(monitor) = MONITOR.get() {
                        add_tray_icon(hwnd, monitor);
                        update_tray_icon(hwnd, monitor);
                    }
                    return LRESULT(0);
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

fn main() {
    unsafe {
        let class_name = "BattestyWindow\0".encode_utf16().collect::<Vec<u16>>();
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(PCWSTR::null()).unwrap().into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..std::mem::zeroed()
        };
        
        RegisterClassW(&wc);
        
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR("Battesty\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            GetModuleHandleW(PCWSTR::null()).unwrap(),
            None,
        );
        
        ShowWindow(hwnd, SW_HIDE);
        
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}