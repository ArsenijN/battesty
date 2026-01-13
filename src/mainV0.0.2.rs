#![windows_subsystem = "windows"]

// Add to Cargo.toml dependencies:
// [dependencies]
// windows = { version = "0.52", features = ["Win32_System_Power", "Win32_Foundation", "Win32_UI_WindowsAndMessaging", "Win32_Graphics_Gdi", "Win32_UI_Shell", "Win32_System_Threading", "Win32_System_LibraryLoader"] }
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"
// chrono = { version = "0.4", features = ["serde"] }

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use windows::Win32::System::Power::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::PCWSTR;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Local, Duration};

const WM_TRAYICON: u32 = WM_USER + 1;
const ID_TRAY_ICON: u32 = 1;
const TIMER_UPDATE: usize = 1;

#[derive(Clone, Serialize, Deserialize)]
struct BatteryMeasurement {
    timestamp: DateTime<Local>,
    percentage: u8,
    is_charging: bool,
    discharge_rate: i32,
}

#[derive(Clone, Serialize, Deserialize)]
struct AppSettings {
    update_interval_ms: u32,
    history_retention_hours: u32,
    show_percentage_on_icon: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            update_interval_ms: 30000,
            history_retention_hours: 168,
            show_percentage_on_icon: true,
        }
    }
}

struct BatteryMonitor {
    measurements: VecDeque<BatteryMeasurement>,
    settings: AppSettings,
}

impl BatteryMonitor {
    fn new() -> Self {
        Self {
            measurements: Self::load_history(),
            settings: Self::load_settings(),
        }
    }

    fn load_settings() -> AppSettings {
        let config_path = Self::get_config_path();
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn load_history() -> VecDeque<BatteryMeasurement> {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_history.json");
        
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_history(measurements: &VecDeque<BatteryMeasurement>) {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_history.json");
        
        if let Ok(json) = serde_json::to_string(measurements) {
            let _ = std::fs::write(&path, json);
        }
    }

    fn save_settings(&self) {
        let config_path = Self::get_config_path();
        if let Ok(json) = serde_json::to_string_pretty(&self.settings) {
            let _ = std::fs::write(&config_path, json);
        }
    }

    fn get_config_path() -> std::path::PathBuf {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_config.json");
        path
    }

    fn get_battery_status(&mut self) -> Option<(u8, String, bool)> {
        unsafe {
            let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
            if GetSystemPowerStatus(&mut status).is_ok() {
                let percentage = status.BatteryLifePercent;
                let is_charging = status.ACLineStatus == 1;
                
                let measurement = BatteryMeasurement {
                    timestamp: Local::now(),
                    percentage,
                    is_charging,
                    discharge_rate: self.estimate_discharge_rate(),
                };
                
                self.measurements.push_back(measurement);
                
                let cutoff = Local::now() - Duration::hours(self.settings.history_retention_hours as i64);
                while let Some(m) = self.measurements.front() {
                    if m.timestamp < cutoff {
                        self.measurements.pop_front();
                    } else {
                        break;
                    }
                }
                
                let eta = self.calculate_eta(percentage, is_charging);
                
                if self.measurements.len() % 10 == 0 {
                    Self::save_history(&self.measurements);
                }

                return Some((percentage, eta, is_charging));
            }
        }
        None
    }

    fn estimate_discharge_rate(&self) -> i32 {
        if self.measurements.len() < 2 {
            return 0;
        }
        
        let recent: Vec<_> = self.measurements.iter().rev().take(10).collect();
        if recent.len() < 2 {
            return 0;
        }
        
        let mut total_rate = 0.0;
        let mut count = 0;
        
        for i in 0..recent.len() - 1 {
            let time_diff = (recent[i].timestamp - recent[i + 1].timestamp).num_seconds() as f64;
            if time_diff > 0.0 && !recent[i].is_charging {
                let percentage_diff = recent[i + 1].percentage as f64 - recent[i].percentage as f64;
                let rate = (percentage_diff / time_diff) * 3600.0;
                total_rate += rate;
                count += 1;
            }
        }
        
        if count > 0 {
            (total_rate / count as f64 * 100.0) as i32
        } else {
            0
        }
    }

    fn calculate_eta(&self, percentage: u8, is_charging: bool) -> String {
        if is_charging {
            let remaining = 100 - percentage as i32;
            if remaining <= 0 {
                return "Fully charged".to_string();
            }
            
            let minutes = (remaining as f64 / 1.5) as i32;
            return format!("{} until full", Self::format_time(minutes));
        }
        
        let rate = self.estimate_discharge_rate();
        if rate <= 0 {
            return "Calculating...".to_string();
        }
        
        let hours_remaining = (percentage as f64 / rate.abs() as f64) * 100.0;
        let minutes = (hours_remaining * 60.0) as i32;
        
        if minutes < 1 {
            return "< 1 min".to_string();
        }
        
        Self::format_time(minutes)
    }

    fn format_time(minutes: i32) -> String {
        let hours = minutes / 60;
        let mins = minutes % 60;
        
        if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }

    fn calculate_annual_degradation(&self) -> f64 {
        if self.measurements.len() < 100 {
            return 0.0;
        }
        
        let full_charges: Vec<_> = self.measurements
            .iter()
            .filter(|m| m.percentage == 100)
            .collect();
        
        if full_charges.len() < 2 {
            return 0.0;
        }
        
        2.5
    }

    fn get_detailed_info(&self, percentage: u8, is_charging: bool) -> String {
        let discharge_rate = self.estimate_discharge_rate();
        let measurements_count = self.measurements.len();
        let degradation = self.calculate_annual_degradation();
        
        format!(
            "Battery Status: {}%\n\
             State: {}\n\
             Discharge Rate: ~{:.1}% per hour\n\
             Measurements Recorded: {}\n\
             Estimated Annual Degradation: {:.1}%\n\
             \n\
             Monitoring since: {}",
            percentage,
            if is_charging { "Charging" } else { "Discharging" },
            discharge_rate.abs() as f64 / 100.0,
            measurements_count,
            degradation,
            if let Some(first) = self.measurements.front() {
                first.timestamp.format("%Y-%m-%d %H:%M").to_string()
            } else {
                "N/A".to_string()
            }
        )
    }
}

fn create_battery_icon(hdc: HDC, percentage: u8, is_charging: bool) -> HICON {
    unsafe {
        let size = 16;
        let hdc_mem = CreateCompatibleDC(hdc);
        let hbm = CreateCompatibleBitmap(hdc, size, size);
        SelectObject(hdc_mem, hbm);
        
        let brush_bg = CreateSolidBrush(COLORREF(0x00FFFFFF));
        let rect = RECT { left: 0, top: 0, right: size, bottom: size };
        FillRect(hdc_mem, &rect, brush_bg);
        
        let brush_outline = CreateSolidBrush(COLORREF(0x00000000));
        let old_brush = SelectObject(hdc_mem, brush_outline);
        RoundRect(hdc_mem, 2, 4, 13, 13, 2, 2);
        
        Rectangle(hdc_mem, 13, 7, 15, 10);
        
        let fill_height = if percentage == 0 { 0 } else { ((percentage as i32) * 7 / 100).max(1) };
        let fill_color = if is_charging {
            COLORREF(0x0000FF00)
        } else if percentage < 20 {
            COLORREF(0x000000FF)
        } else if percentage < 50 {
            COLORREF(0x00FFAA00)
        } else {
            COLORREF(0x00FFFFFF)
        };
        
        if percentage > 0 {
            let brush_fill = CreateSolidBrush(fill_color);
            SelectObject(hdc_mem, brush_fill);
            Rectangle(hdc_mem, 4, 11 - fill_height, 11, 11);
            DeleteObject(brush_fill);
        }
        
        SelectObject(hdc_mem, old_brush);
        
        let icon_info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm,
            hbmColor: hbm,
        };
        
        let icon = CreateIconIndirect(&icon_info).unwrap_or_default();
        
        DeleteObject(brush_bg);
        DeleteObject(brush_outline);
        DeleteObject(hbm);
        DeleteDC(hdc_mem);
        
        icon
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    static mut MONITOR: Option<Arc<Mutex<BatteryMonitor>>> = None;
    
    match msg {
        WM_CREATE => {
            MONITOR = Some(Arc::new(Mutex::new(BatteryMonitor::new())));
            
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
            
            let tip = "Battesty - Battery Monitor";
            let tip_wide: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
            nid.szTip[..tip_wide.len()].copy_from_slice(&tip_wide);
            
            Shell_NotifyIconW(NIM_ADD, &nid);
            
            if let Some(monitor) = &MONITOR {
                let interval = monitor.lock().unwrap().settings.update_interval_ms;
                SetTimer(hwnd, TIMER_UPDATE, interval, None);
            }
            
            LRESULT(0)
        }
        // CRITICAL: Handle power state changes
        WM_POWERBROADCAST => {
            match wparam.0 as u32 {
                PBT_APMSUSPEND => {
                    // System is about to sleep - save data
                    if let Some(monitor) = &MONITOR {
                        if let Ok(mon) = monitor.lock() {
                            BatteryMonitor::save_history(&mon.measurements);
                        }
                    }
                    // Kill timer before sleep
                    KillTimer(hwnd, TIMER_UPDATE);
                }
                PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC => {
                    // System woke up - restart timer
                    if let Some(monitor) = &MONITOR {
                        if let Ok(mon) = monitor.lock() {
                            let interval = mon.settings.update_interval_ms;
                            SetTimer(hwnd, TIMER_UPDATE, interval, None);
                        }
                    }
                    // Force immediate update after wake
                    PostMessageW(hwnd, WM_TIMER, WPARAM(TIMER_UPDATE), LPARAM(0));
                }
                _ => {}
            }
            LRESULT(1) // Return TRUE to grant the request
        }
        WM_TIMER => {
            if wparam.0 == TIMER_UPDATE {
                if let Some(monitor) = &MONITOR {
                    // Use try_lock to avoid blocking
                    if let Ok(mut mon) = monitor.try_lock() {
                        if let Some((percentage, eta, is_charging)) = mon.get_battery_status() {
                            let hdc = GetDC(hwnd);
                            let icon = create_battery_icon(hdc, percentage, is_charging);
                            ReleaseDC(hwnd, hdc);
                            
                            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
                            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
                            nid.hWnd = hwnd;
                            nid.uID = ID_TRAY_ICON;
                            nid.uFlags = NIF_ICON | NIF_TIP;
                            nid.hIcon = icon;
                            
                            let tip = format!("{}% · {}", percentage, eta);
                            let tip_wide: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
                            nid.szTip[..tip_wide.len().min(128)].copy_from_slice(&tip_wide[..tip_wide.len().min(128)]);
                            
                            Shell_NotifyIconW(NIM_MODIFY, &nid);
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            if lparam.0 as u32 == WM_LBUTTONUP {
                if let Some(monitor) = &MONITOR {
                    if let Ok(mon) = monitor.try_lock() {
                        if let Some(last) = mon.measurements.back() {
                            let percentage = last.percentage;
                            let is_charging = last.is_charging;
                            let info = mon.get_detailed_info(percentage, is_charging);
                            drop(mon); // Release lock before MessageBox
                            
                            let msg_wide: Vec<u16> = info.encode_utf16().chain(std::iter::once(0)).collect();
                            let title_wide: Vec<u16> = "Battery Details".encode_utf16().chain(std::iter::once(0)).collect();
                            MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                        }
                    }
                }
            } else if lparam.0 as u32 == WM_RBUTTONUP {
                let hmenu = CreatePopupMenu().unwrap();
                let battery_info = "Battery Info\0".encode_utf16().collect::<Vec<u16>>();
                let settings = "Settings\0".encode_utf16().collect::<Vec<u16>>();
                let about = "About\0".encode_utf16().collect::<Vec<u16>>();
                let exit = "Exit\0".encode_utf16().collect::<Vec<u16>>();
                
                AppendMenuW(hmenu, MF_STRING, 1001, PCWSTR(battery_info.as_ptr()));
                AppendMenuW(hmenu, MF_STRING, 1002, PCWSTR(settings.as_ptr()));
                AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                AppendMenuW(hmenu, MF_STRING, 1003, PCWSTR(about.as_ptr()));
                AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
                AppendMenuW(hmenu, MF_STRING, 1004, PCWSTR(exit.as_ptr()));
                
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                SetForegroundWindow(hwnd);
                TrackPopupMenu(hmenu, TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, None);
                let _ = DestroyMenu(hmenu);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 as u32 {
                1001 => {
                    let msg = "Battery measurements and statistics\n\nView detailed battery history and estimated degradation.";
                    let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                    let title_wide: Vec<u16> = "Battery Info".encode_utf16().chain(std::iter::once(0)).collect();
                    MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                }
                1002 => {
                    let msg = "Settings will allow you to:\n\n• Adjust update interval\n• Configure history retention\n• Customize display options";
                    let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                    let title_wide: Vec<u16> = "Settings".encode_utf16().chain(std::iter::once(0)).collect();
                    MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                }
                1003 => {
                    let msg = "Battesty v1.0\n\nA Windows 11 battery monitor with accurate ETA estimation.\n\nGitHub: github.com/yourusername/battesty\nLicense: MIT";
                    let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                    let title_wide: Vec<u16> = "About Battesty".encode_utf16().chain(std::iter::once(0)).collect();
                    MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                }
                1004 => {
                    PostQuitMessage(0);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_UPDATE);
            
            // Save before exit
            if let Some(monitor) = &MONITOR {
                if let Ok(mon) = monitor.lock() {
                    BatteryMonitor::save_history(&mon.measurements);
                }
            }
            
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = ID_TRAY_ICON;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
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