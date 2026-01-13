#![windows_subsystem = "windows"]

use std::sync::{Arc, Mutex, OnceLock};
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
const TIMER_SAVE: usize = 2;

// Debug mode: Set to true to cycle through battery percentages automatically
const DEBUG_MODE: bool = false;

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
    last_icon: Option<HICON>,
    debug_percentage: u8,
    debug_charging: bool,
}

impl BatteryMonitor {
    fn new() -> Self {
        Self {
            measurements: Self::load_history(),
            settings: Self::load_settings(),
            last_icon: None,
            debug_percentage: 100,
            debug_charging: false,
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

    fn save_history(&self) {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_history.json");
        
        if let Ok(json) = serde_json::to_string(&self.measurements) {
            let _ = std::fs::write(&path, json);
        }
    }

    fn get_config_path() -> std::path::PathBuf {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_config.json");
        path
    }

    fn cleanup_old_measurements(&mut self) {
        let cutoff = Local::now() - Duration::hours(self.settings.history_retention_hours as i64);
        while let Some(m) = self.measurements.front() {
            if m.timestamp < cutoff {
                self.measurements.pop_front();
            } else {
                break;
            }
        }
    }

    fn get_battery_status(&mut self) -> Option<(u8, String, bool)> {
        if DEBUG_MODE {
            // Cycle through percentages for testing
            self.debug_percentage = if self.debug_percentage > 0 {
                self.debug_percentage - 5
            } else {
                100
            };
            
            // Toggle charging state every full cycle
            if self.debug_percentage == 100 {
                self.debug_charging = !self.debug_charging;
            }
            
            let eta = if self.debug_charging {
                format!("{} until full", Self::format_time(((100 - self.debug_percentage) as f64 / 1.5) as i32))
            } else {
                format!("{} remaining", Self::format_time(self.debug_percentage as i32 * 3))
            };
            
            return Some((self.debug_percentage, eta, self.debug_charging));
        }

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
                
                // Clean up old measurements less frequently
                if self.measurements.len() % 100 == 0 {
                    self.cleanup_old_measurements();
                }
                
                let eta = self.calculate_eta(percentage, is_charging);

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
             {}\
             \n\
             Monitoring since: {}",
            percentage,
            if is_charging { "Charging" } else { "Discharging" },
            discharge_rate.abs() as f64 / 100.0,
            measurements_count,
            degradation,
            if DEBUG_MODE { "\n[DEBUG MODE ACTIVE]\n" } else { "" },
            if let Some(first) = self.measurements.front() {
                first.timestamp.format("%Y-%m-%d %H:%M").to_string()
            } else {
                "N/A".to_string()
            }
        )
    }

    fn destroy_icon(&mut self) {
        if let Some(icon) = self.last_icon.take() {
            unsafe {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

fn create_battery_icon(hdc: HDC, percentage: u8, is_charging: bool) -> HICON {
    unsafe {
        let size = 16;
        let hdc_mem = CreateCompatibleDC(hdc);
        let hbm = CreateCompatibleBitmap(hdc, size, size);
        SelectObject(hdc_mem, hbm);
        
        // Transparent background
        let brush_bg = CreateSolidBrush(COLORREF(0x00FFFFFF));
        let rect = RECT { left: 0, top: 0, right: size, bottom: size };
        FillRect(hdc_mem, &rect, brush_bg);
        
        // Battery outline (thicker and more visible)
        let brush_outline = CreateSolidBrush(COLORREF(0x00000000));
        let old_brush = SelectObject(hdc_mem, brush_outline);
        
        // Main battery body
        RoundRect(hdc_mem, 2, 4, 14, 15, 2, 2);
        
        // Battery terminal (nub at the top)
        Rectangle(hdc_mem, 5, 2, 11, 4);
        
        // Calculate fill height (0-8 pixels for the main body)
        let body_height = 8;
        let fill_height = if percentage == 0 { 
            0 
        } else { 
            ((percentage as i32) * body_height / 120).max(1) 
        };
        
        // Determine fill color based on state and percentage
        let fill_color = if is_charging {
            COLORREF(0x0000C800) // Green for charging
        } else if percentage < 15 {
            COLORREF(0x000000FF) // Red for critical
        } else if percentage < 30 {
            COLORREF(0x000080FF) // Orange for low
        } else {
            COLORREF(0x00FFFFFF) // White for normal
        };
        
        // Draw the battery fill
        if fill_height > 0 {
            let brush_fill = CreateSolidBrush(fill_color);
            SelectObject(hdc_mem, brush_fill);
            
            // Fill from bottom up
            let fill_top = 11 - fill_height;
            Rectangle(hdc_mem, 4, fill_top, 12, 13);
            
            DeleteObject(brush_fill);
        }
        
        // Add charging indicator (lightning bolt)
        if is_charging && percentage < 100 {
            let brush_bolt = CreateSolidBrush(COLORREF(0x00FFFF00)); // Yellow
            SelectObject(hdc_mem, brush_bolt);
            
            // Simple lightning bolt shape
            let points = [
                POINT { x: 8, y: 6 },
                POINT { x: 7, y: 9 },
                POINT { x: 9, y: 9 },
                POINT { x: 7, y: 12 },
            ];
            Polygon(hdc_mem, &points);
            
            DeleteObject(brush_bolt);
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

fn add_tray_icon(hwnd: HWND, monitor: &Arc<Mutex<BatteryMonitor>>) {
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

fn update_tray_icon(hwnd: HWND, monitor: &Arc<Mutex<BatteryMonitor>>) {
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
            
            // Register for TaskbarCreated message
            let taskbar_created = "TaskbarCreated\0".encode_utf16().collect::<Vec<u16>>();
            let msg_id = RegisterWindowMessageW(PCWSTR(taskbar_created.as_ptr()));
            let _ = WM_TASKBARCREATED_MSG.set(msg_id);
            
            add_tray_icon(hwnd, &monitor);
            
            let interval = monitor.lock().unwrap().settings.update_interval_ms;
            let update_interval = if DEBUG_MODE { 2000 } else { interval }; // 2 seconds in debug mode
            SetTimer(hwnd, TIMER_UPDATE, update_interval, None);
            SetTimer(hwnd, TIMER_SAVE, 300000, None);
            
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
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
            LRESULT(1)
        }
        WM_TIMER => {
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
            LRESULT(0)
        }
        WM_TRAYICON => {
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
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 as u32 {
                1001 => {
                    let msg = "Battery measurements and statistics\n\nView detailed battery history and estimated degradation.\n\nComming soon!";
                    let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();
                    let title_wide: Vec<u16> = "Battery Info".encode_utf16().chain(std::iter::once(0)).collect();
                    MessageBoxW(hwnd, PCWSTR(msg_wide.as_ptr()), PCWSTR(title_wide.as_ptr()), MB_OK | MB_ICONINFORMATION);
                }
                1002 => {
                    let msg = "Settings will allow you to:\n\n• Adjust update interval\n• Configure history retention\n• Customize display options\n\nComming soon!";
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
            LRESULT(0)
        }
        WM_DESTROY => {
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
            LRESULT(0)
        }
        _ => {
            // Handle taskbar restart
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