use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::Graphics::Gdi::*;
use windows::core::PCWSTR;

use crate::battery::{BatteryMonitor, DEBUG_MODE};
use crate::icon::create_battery_icon;
use crate::{MONITOR, WM_TRAYICON, ID_TRAY_ICON, TIMER_UPDATE, TIMER_SAVE};

pub fn add_tray_icon(hwnd: HWND, monitor: &Arc<Mutex<BatteryMonitor>>) {
    unsafe {
        let hdc = GetDC(hwnd);
        let icon = create_battery_icon(hdc, 50, false);
        ReleaseDC(hwnd, hdc);
        
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = ID_TRAY_ICON;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = icon;
        
        let tip = if DEBUG_MODE {
            "Battesty [DEBUG] - Battery Monitor"
        } else {
            "Battesty - Battery Monitor"
        };
        let tip_wide: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
        nid.szTip[..tip_wide.len()].copy_from_slice(&tip_wide);
        
        Shell_NotifyIconW(NIM_ADD, &nid);
        
        if let Ok(mut mon) = monitor.lock() {
            mon.destroy_icon();
            mon.last_icon = Some(icon);
        }
    }
}

pub fn update_tray_icon(hwnd: HWND, monitor: &Arc<Mutex<BatteryMonitor>>) {
    if let Ok(mut mon) = monitor.lock() {
        if let Some((percentage, eta, is_charging)) = mon.get_battery_status() {
            unsafe {
                let hdc = GetDC(hwnd);
                let icon = create_battery_icon(hdc, percentage, is_charging);
                ReleaseDC(hwnd, hdc);
                
                let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
                nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = hwnd;
                nid.uID = ID_TRAY_ICON;
                nid.uFlags = NIF_ICON | NIF_TIP;
                nid.hIcon = icon;
                
                let tip = if DEBUG_MODE {
                    format!("[DEBUG] {}% · {}", percentage, eta)
                } else {
                    format!("{}% · {}", percentage, eta)
                };
                let tip_wide: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
                nid.szTip[..tip_wide.len().min(128)].copy_from_slice(&tip_wide[..tip_wide.len().min(128)]);
                
                Shell_NotifyIconW(NIM_MODIFY, &nid);
                
                mon.destroy_icon();
                mon.last_icon = Some(icon);
            }
        }
    }
}

pub fn handle_power_event(wparam: WPARAM, hwnd: HWND) {
    use windows::Win32::System::Power::*;
    
    match wparam.0 as u32 {
        PBT_APMSUSPEND => {
            if let Some(monitor) = MONITOR.get() {
                if let Ok(mon) = monitor.lock() {
                    mon.save_history();
                }
            }
        }
        PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => {
            if let Some(monitor) = MONITOR.get() {
                update_tray_icon(hwnd, monitor);
            }
        }
        _ => {}
    }
}

pub fn handle_timer_event(wparam: WPARAM, hwnd: HWND) {
    if wparam.0 == TIMER_UPDATE {
        if let Some(monitor) = MONITOR.get() {
            update_tray_icon(hwnd, monitor);
        }
    } else if wparam.0 == TIMER_SAVE {
        if let Some(monitor) = MONITOR.get() {
            if let Ok(mon) = monitor.lock() {
                mon.save_history();
            }
        }
    }
}

pub fn handle_tray_event(lparam: LPARAM, hwnd: HWND) {
    unsafe {
        if lparam.0 as u32 == WM_LBUTTONUP {
            if let Some(monitor) = MONITOR.get() {
                if let Ok(mon) = monitor.lock() {
                    if let Some(last) = mon.measurements.back() {
                        let percentage = last.percentage;
                        let is_charging = last.is_charging;
                        let info = mon.get_detailed_info(percentage, is_charging);
                        drop(mon);
                        
                        let msg_wide: Vec<u16> = info.encode_utf16().chain(std::iter::once(0)).collect();
                        let title_wide: Vec<u16> = "Battery Details".encode_utf16().chain(std::iter::once(0)).collect();
                        MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                    }
                }
            }
        } else if lparam.0 as u32 == WM_RBUTTONUP {
            show_context_menu(hwnd);
        }
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let hmenu = CreatePopupMenu().unwrap();
        let battery_info = "Battery Info\0".encode_utf16().collect::<Vec<u16>>();
        let settings = "Settings\0".encode_utf16().collect::<Vec<u16>>();
        let about = "About\0".encode_utf16().collect::<Vec<u16>>();
        let exit = "Exit\0".encode_utf16().collect::<Vec<u16>>();
        
        let _ = AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(battery_info.as_ptr()));
        let _ = AppendMenuW(hmenu, MF_STRING, 1002, PCWSTR(settings.as_ptr()));
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hmenu, MF_STRING, 1003, PCWSTR(about.as_ptr()));
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(hmenu, MF_STRING, 1004, PCWSTR(exit.as_ptr()));
        
        let mut pt = POINT { x: 0, y: 0 };
        let _ = GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(hmenu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(hmenu);
    }
}

pub fn handle_menu_command(wparam: WPARAM, hwnd: HWND) {
    unsafe {
        match wparam.0 as u32 {
            1001 => {
                let msg = "Battery measurements and statistics\n\nView detailed battery history and estimated degradation.\n\nComing soon!";
                let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                let title_wide: Vec<u16> = "Battery Info".encode_utf16().chain(std::iter::once(0)).collect();
                MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
            }
            1002 => {
                let msg = "Settings will allow you to:\n\n• Adjust update interval\n• Configure history retention\n• Customize display options\n\nComing soon!";
                let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                let title_wide: Vec<u16> = "Settings".encode_utf16().chain(std::iter::once(0)).collect();
                MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
            }
            1003 => {
                let msg = "Battesty v1.0\n\nA Windows 11 battery monitor with accurate ETA estimation.\n\nGitHub: https://github.com/ArsenijN/battesty\nLicense: MIT, see LICENSE.md";
                let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                let title_wide: Vec<u16> = "About Battesty".encode_utf16().chain(std::iter::once(0)).collect();
                MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
            }
            1004 => {
                PostQuitMessage(0);
            }
            _ => {}
        }
    }
}

pub fn cleanup_and_exit(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_UPDATE);
        let _ = KillTimer(hwnd, TIMER_SAVE);
        
        if let Some(monitor) = MONITOR.get() {
            if let Ok(mut mon) = monitor.lock() {
                mon.save_history();
                mon.destroy_icon();
            }
        }
        
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = ID_TRAY_ICON;
        Shell_NotifyIconW(NIM_DELETE, &nid);
        
        PostQuitMessage(0);
    }
}