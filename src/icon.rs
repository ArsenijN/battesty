use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;

// High-res icon support for modern displays
const ICON_SIZE: i32 = 32; // 32x32 for better quality on high-DPI displays

pub fn create_battery_icon(hdc: HDC, percentage: u8, is_charging: bool) -> HICON {
    unsafe {
        let hdc_mem = CreateCompatibleDC(hdc);
        let hbm = CreateCompatibleBitmap(hdc, ICON_SIZE, ICON_SIZE);
        SelectObject(hdc_mem, hbm);
        
        // Transparent background
        let brush_bg = CreateSolidBrush(COLORREF(0x00FFFFFF));
        let rect = RECT { left: 0, top: 0, right: ICON_SIZE, bottom: ICON_SIZE };
        FillRect(hdc_mem, &rect, brush_bg);
        
        // Scale coordinates for 32x32 canvas
        let scale = 2;
        
        // Battery outline (black)
        let brush_outline = CreateSolidBrush(COLORREF(0x00000000));
        let old_brush = SelectObject(hdc_mem, brush_outline);
        
        // Main battery body
        RoundRect(hdc_mem, 4*scale, 8*scale, 28*scale, 30*scale, 3*scale, 3*scale);
        
        // Battery terminal (nub at the top)
        Rectangle(hdc_mem, 10*scale, 4*scale, 22*scale, 8*scale);
        
        // Calculate fill height
        let body_height = 20 * scale;
        let fill_height = if percentage == 0 { 
            0 
        } else { 
            ((percentage as i32) * body_height / 100).max(scale) 
        };
        
        // Determine fill color
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
            let fill_top = 28*scale - fill_height;
            Rectangle(hdc_mem, 6*scale, fill_top, 26*scale, 28*scale);
            
            DeleteObject(brush_fill);
        }
        
        // Add fully charged indicator (green knob at top)
        if percentage >= 90 && is_charging {
            let brush_green = CreateSolidBrush(COLORREF(0x0000FF00)); // Bright green
            SelectObject(hdc_mem, brush_green);
            
            // Small circle at the battery terminal
            Ellipse(hdc_mem, 14*scale, 2*scale, 18*scale, 6*scale);
            
            DeleteObject(brush_green);
        }
        
        // Add charging indicator (lightning bolt) - BLACK not yellow
        if is_charging && percentage < 100 {
            let brush_bolt = CreateSolidBrush(COLORREF(0x00000000)); // Black bolt
            SelectObject(hdc_mem, brush_bolt);
            
            // Larger lightning bolt shape
            let points = [
                POINT { x: 16*scale, y: 12*scale },
                POINT { x: 14*scale, y: 18*scale },
                POINT { x: 18*scale, y: 18*scale },
                POINT { x: 14*scale, y: 24*scale },
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