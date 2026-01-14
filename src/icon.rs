use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;

// High-res icon support for modern displays
const ICON_SIZE: i32 = 32; // 32x32 for better quality on high-DPI displays

pub fn create_battery_icon(hdc: HDC, percentage: u8, is_charging: bool) -> HICON {
    unsafe {
        let hdc_mem = CreateCompatibleDC(hdc);
        let hbm = CreateCompatibleBitmap(hdc, ICON_SIZE, ICON_SIZE);
        let hbm_mask = CreateCompatibleBitmap(hdc, ICON_SIZE, ICON_SIZE);
        SelectObject(hdc_mem, hbm);
        
        // White background (will be made transparent via mask)
        let brush_bg = CreateSolidBrush(COLORREF(0x00FFFFFF));
        let rect = RECT { left: 0, top: 0, right: ICON_SIZE, bottom: ICON_SIZE };
        FillRect(hdc_mem, &rect, brush_bg);
        DeleteObject(brush_bg);
        
        // Use thin pen for outline
        let pen_outline = CreatePen(PS_SOLID, 2, COLORREF(0x00000000));
        let pen_null = GetStockObject(NULL_PEN);
        let old_pen = SelectObject(hdc_mem, pen_outline);
        let brush_outline = GetStockObject(NULL_BRUSH);
        let old_brush = SelectObject(hdc_mem, brush_outline);
        
        // Main battery body - outline only
        RoundRect(hdc_mem, 6, 10, 26, 28, 3, 3);
        
        // Battery terminal (nub at the top) - outline only
        Rectangle(hdc_mem, 12, 6, 20, 10);
        
        // Calculate fill height
        let body_height = 16;
        let fill_height = if percentage == 0 { 
            0 
        } else { 
            ((percentage as i32) * body_height / 100).max(1) 
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
        
        // Draw the battery fill WITHOUT border
        if fill_height > 0 {
            let brush_fill = CreateSolidBrush(fill_color);
            SelectObject(hdc_mem, brush_fill);
            SelectObject(hdc_mem, pen_null); // Use NULL pen for fill - no border
            
            // Fill from bottom up (inside the battery body)
            let fill_top = 26 - fill_height;
            Rectangle(hdc_mem, 8, fill_top, 24, 26);
            
            DeleteObject(brush_fill);
        }
        
        // Add fully charged indicator (green circle at top)
        if percentage >= 90 && is_charging {
            let brush_green = CreateSolidBrush(COLORREF(0x0000FF00));
            SelectObject(hdc_mem, brush_green);
            SelectObject(hdc_mem, pen_null); // NULL pen for the circle
            
            Ellipse(hdc_mem, 14, 4, 18, 8);
            
            DeleteObject(brush_green);
        }
        
        // Add charging indicator (lightning bolt) - solid fill, thin outer border
        if is_charging && percentage < 100 {
            let brush_bolt = CreateSolidBrush(COLORREF(0x00000000)); // Black fill
            SelectObject(hdc_mem, brush_bolt);
            SelectObject(hdc_mem, pen_null); // NULL pen for bolt body
            
            // Lightning bolt shape
            let points = [
                POINT { x: 16, y: 14 },
                POINT { x: 14, y: 19 },
                POINT { x: 17, y: 19 },
                POINT { x: 15, y: 24 },
            ];
            Polygon(hdc_mem, &points);
            
            DeleteObject(brush_bolt);
        }
        
        SelectObject(hdc_mem, old_brush);
        SelectObject(hdc_mem, old_pen);
        DeleteObject(pen_outline);
        
        let icon_info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm,
        };
        
        let icon = CreateIconIndirect(&icon_info).unwrap_or_default();
        
        DeleteObject(hbm);
        DeleteObject(hbm_mask);
        DeleteDC(hdc_mem);
        
        icon
    }
}