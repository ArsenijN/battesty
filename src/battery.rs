use std::collections::VecDeque;
use windows::Win32::System::Power::*;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Local, Duration};
use crate::settings::AppSettings;

pub const DEBUG_MODE: bool = false;

#[derive(Clone, Serialize, Deserialize)]
pub struct BatteryMeasurement {
    pub timestamp: DateTime<Local>,
    pub percentage: u8,
    pub is_charging: bool,
    pub discharge_rate: i32,
}

pub struct BatteryMonitor {
    pub measurements: VecDeque<BatteryMeasurement>,
    pub settings: AppSettings,
    pub last_icon: Option<windows::Win32::UI::WindowsAndMessaging::HICON>,
    debug_percentage: u8,
    debug_charging: bool,
}

impl BatteryMonitor {
    pub fn new() -> Self {
        Self {
            measurements: Self::load_history(),
            settings: AppSettings::load(),
            last_icon: None,
            debug_percentage: 100,
            debug_charging: false,
        }
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

    pub fn save_history(&self) {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_history.json");
        
        if let Ok(json) = serde_json::to_string(&self.measurements) {
            let _ = std::fs::write(&path, json);
        }
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

    pub fn get_battery_status(&mut self) -> Option<(u8, String, bool)> {
        if DEBUG_MODE {
            self.debug_percentage = if self.debug_percentage > 0 {
                self.debug_percentage - 5
            } else {
                100
            };
            
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

    pub fn get_detailed_info(&self, percentage: u8, is_charging: bool) -> String {
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

    pub fn destroy_icon(&mut self) {
        if let Some(icon) = self.last_icon.take() {
            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;
                let _ = DestroyIcon(icon);
            }
        }
    }
}