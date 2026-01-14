use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;

const CANVAS_SIZE: i32 = 64; // 16x16 base (scales to 64x64 for taskbar)

// Convert relative coordinates (0.0-1.0) to canvas pixels
#[inline]
fn rel(val: f32, canvas: i32) -> i32 {
    (val * canvas as f32).round() as i32
}

pub fn create_battery_icon(hdc: HDC, percentage: u8, is_charging: bool) -> HICON {
    unsafe {
        let hdc_mem = CreateCompatibleDC(hdc);
        let hbm = CreateCompatibleBitmap(hdc, CANVAS_SIZE, CANVAS_SIZE);
        let hbm_mask = CreateCompatibleBitmap(hdc, CANVAS_SIZE, CANVAS_SIZE);
        SelectObject(hdc_mem, hbm);
        
        // === Create mask bitmap for transparency ===
        // White in mask = transparent, Black in mask = opaque
        let hdc_mask = CreateCompatibleDC(hdc);
        SelectObject(hdc_mask, hbm_mask);
        let brush_mask_white = CreateSolidBrush(COLORREF(0x00FFFFFF)); // White = transparent
        let rect = RECT { left: 0, top: 0, right: CANVAS_SIZE, bottom: CANVAS_SIZE };
        FillRect(hdc_mask, &rect, brush_mask_white);
        DeleteObject(brush_mask_white);
        
        // === Transparent background (not white) ===
        let brush_bg = CreateSolidBrush(COLORREF(0x00000000)); // Black for transparent areas
        FillRect(hdc_mem, &rect, brush_bg);
        DeleteObject(brush_bg);
        
        let c = CANVAS_SIZE;
        
        // === Draw Battery Body (vector outline) ===
        let pen_outline = CreatePen(PS_SOLID, 1, COLORREF(0x00FFFFFF)); // White outline
        let old_pen = SelectObject(hdc_mem, pen_outline);
        let brush_null = GetStockObject(NULL_BRUSH);
        let old_brush = SelectObject(hdc_mem, brush_null);
        
        // Battery body polygon (from GIMP 16x16 coords, relative coords)
        // (2,2), (5,2), (5,1), (10,1), (10,2), (13,2), (13,14), (2,14)
        let battery_points = [
            POINT { x: rel(2.0/16.0, c), y: rel(2.0/16.0, c) },      // (2,2)
            POINT { x: rel(5.0/16.0, c), y: rel(2.0/16.0, c) },      // (5,2)
            POINT { x: rel(5.0/16.0, c), y: rel(1.0/16.0, c) },      // (5,1)
            POINT { x: rel(10.0/16.0, c), y: rel(1.0/16.0, c) },     // (10,1)
            POINT { x: rel(10.0/16.0, c), y: rel(2.0/16.0, c) },     // (10,2)
            POINT { x: rel(13.0/16.0, c), y: rel(2.0/16.0, c) },     // (13,2)
            POINT { x: rel(13.0/16.0, c), y: rel(14.0/16.0, c) },    // (13,14)
            POINT { x: rel(2.0/16.0, c), y: rel(14.0/16.0, c) },     // (2,14)
        ];
        Polyline(hdc_mem, &battery_points);
        
        // Close the polygon
        Polyline(hdc_mem, &[
            battery_points[7],
            battery_points[0],
        ]);
        
        // === Draw Fill Level ===
        if percentage > 0 {
            // Determine fill color based on percentage and charging state
            let fill_color = if is_charging {
                COLORREF(0x0000C800) // Green for charging
            } else if percentage < 5 {
                COLORREF(0x000000FF) // Red for urgent (<5%)
            } else if percentage < 15 {
                COLORREF(0x000080FF) // Orange for warning (<15%)
            } else {
                COLORREF(0x00FFFFFF) // White/normal for good
            };
            
            let brush_fill = CreateSolidBrush(fill_color);
            SelectObject(hdc_mem, brush_fill);
            SelectObject(hdc_mem, GetStockObject(NULL_PEN)); // No border on fill
            
            // Fill region bounds (from GIMP): (3,3) to (12,13)
            // Fill from bottom up based on percentage
            let fill_left = rel(3.0/16.0, c);
            let fill_right = rel(13.0/16.0, c);
            let fill_bottom = rel(14.0/16.0, c);
            let fill_top_full = rel(2.0/16.0, c);
            let fill_height = fill_bottom - fill_top_full;
            
            let current_fill_height = (fill_height * percentage as i32 / 100).max(1);
            let fill_top = fill_bottom - current_fill_height;
            
            Rectangle(hdc_mem, fill_left, fill_top, fill_right, fill_bottom);
            
            // Mark fill area as opaque in mask
            let brush_mask_black = CreateSolidBrush(COLORREF(0x00000000));
            SelectObject(hdc_mask, brush_mask_black);
            Rectangle(hdc_mask, fill_left, fill_top, fill_right, fill_bottom);
            DeleteObject(brush_mask_black);
            
            DeleteObject(brush_fill);
        }
        
        // === Draw Battery Outline as Opaque in Mask ===
        let brush_mask_black = CreateSolidBrush(COLORREF(0x00000000));
        SelectObject(hdc_mask, brush_mask_black);
        Polyline(hdc_mask, &battery_points);
        Polyline(hdc_mask, &[battery_points[7], battery_points[0]]);
        DeleteObject(brush_mask_black);
        
        // === Draw Charging Indicator (Lightning Bolt) ===
        if is_charging && percentage < 100 {
            let brush_bolt = CreateSolidBrush(COLORREF(0x0000FFFF)); // Yellow for charging
            SelectObject(hdc_mem, brush_bolt);
            SelectObject(hdc_mem, GetStockObject(NULL_PEN));
            
            // Lightning bolt from GIMP (pixel art coordinates)
            // Using approximation as polygon
            let bolt_points = [
                POINT { x: rel(11.0/16.0, c), y: rel(7.0/16.0, c) },   // Y11,7
                POINT { x: rel(10.0/16.0, c), y: rel(8.0/16.0, c) },   // 10,8
                POINT { x: rel(9.0/16.0, c), y: rel(9.0/16.0, c) },    // 9,9
                POINT { x: rel(8.0/16.0, c), y: rel(10.0/16.0, c) },   // 8,10
                POINT { x: rel(12.0/16.0, c), y: rel(9.0/16.0, c) },   // 12,9
                POINT { x: rel(10.0/16.0, c), y: rel(6.0/16.0, c) },   // Back to top area
            ];
            Polygon(hdc_mem, &bolt_points);
            
            // Mark bolt as opaque in mask
            let brush_mask_black = CreateSolidBrush(COLORREF(0x00000000));
            SelectObject(hdc_mask, brush_mask_black);
            Polygon(hdc_mask, &bolt_points);
            DeleteObject(brush_mask_black);
            
            DeleteObject(brush_bolt);
        }
        
        // === Draw Warning Indicator (5% <= battery < 15%) ===
        if !is_charging && percentage > 0 && percentage < 15 {
            // Step 1: Draw filled black rectangle with black border
            let brush_black = CreateSolidBrush(COLORREF(0x00000000)); // Black fill
            let pen_black = CreatePen(PS_SOLID, 1, COLORREF(0x00000000)); // Black border
            SelectObject(hdc_mem, brush_black);
            SelectObject(hdc_mem, pen_black);
            
            Rectangle(hdc_mem,
                rel(11.0/16.0, c), rel(6.0/16.0, c),   // (11,6)
                rel(13.0/16.0, c), rel(14.0/16.0, c)   // (13,14)
            );
            
            DeleteObject(brush_black);
            DeleteObject(pen_black);
            
            // Step 2: Draw red vertical line (12,7) to (12,11)
            let pen_red = CreatePen(PS_SOLID, 1, COLORREF(0x000000FF)); // Red pen
            SelectObject(hdc_mem, pen_red);
            
            let x = rel(12.0/16.0, c);
            let y_top = rel(7.0/16.0, c);
            let y_bottom = rel(11.0/16.0, c);
            
            MoveToEx(hdc_mem, x, y_top, None);
            LineTo(hdc_mem, x, y_bottom);
            
            DeleteObject(pen_red);
            
            // Step 3: Draw red dot at (12,13)
            let brush_red = CreateSolidBrush(COLORREF(0x000000FF)); // Red
            SelectObject(hdc_mem, brush_red);
            SelectObject(hdc_mem, GetStockObject(NULL_PEN));
            
            let dot_x = rel(12.0/16.0, c);
            let dot_y = rel(13.0/16.0, c);
            Ellipse(hdc_mem, dot_x - 1, dot_y - 1, dot_x + 2, dot_y + 2);
            
            DeleteObject(brush_red);
            
            // Mark as opaque in mask
            let brush_mask_black = CreateSolidBrush(COLORREF(0x00000000));
            SelectObject(hdc_mask, brush_mask_black);
            Rectangle(hdc_mask,
                rel(11.0/16.0, c), rel(6.0/16.0, c),
                rel(13.0/16.0, c), rel(14.0/16.0, c)
            );
            DeleteObject(brush_mask_black);
        }
        
        // === Draw Urgent Indicator (battery < 5%) ===
        if !is_charging && percentage < 5 {
            // Step 1: Draw filled black rectangle with black border (9,6) to (13,14)
            let brush_black = CreateSolidBrush(COLORREF(0x00000000)); // Black fill
            let pen_black = CreatePen(PS_SOLID, 1, COLORREF(0x00000000)); // Black border
            SelectObject(hdc_mem, brush_black);
            SelectObject(hdc_mem, pen_black);
            
            Rectangle(hdc_mem,
                rel(9.0/16.0, c), rel(6.0/16.0, c),    // (9,6)
                rel(13.0/16.0, c), rel(14.0/16.0, c)   // (13,14)
            );
            
            DeleteObject(brush_black);
            DeleteObject(pen_black);
            
            // Step 2: Draw red vertical line (12,7) to (12,11)
            let pen_red = CreatePen(PS_SOLID, 1, COLORREF(0x000000FF)); // Red pen
            SelectObject(hdc_mem, pen_red);
            
            let x1 = rel(12.0/16.0, c);
            let y_top = rel(7.0/16.0, c);
            let y_bottom = rel(11.0/16.0, c);
            
            MoveToEx(hdc_mem, x1, y_top, None);
            LineTo(hdc_mem, x1, y_bottom);
            
            // Step 3: Draw red dot at (12,13)
            let brush_red = CreateSolidBrush(COLORREF(0x000000FF)); // Red
            SelectObject(hdc_mem, brush_red);
            SelectObject(hdc_mem, GetStockObject(NULL_PEN));
            
            let dot_x1 = rel(12.0/16.0, c);
            let dot_y = rel(13.0/16.0, c);
            Ellipse(hdc_mem, dot_x1 - 1, dot_y - 1, dot_x1 + 2, dot_y + 2);
            
            DeleteObject(brush_red);
            
            // Step 4: Draw red vertical line (10,7) to (10,11)
            let pen_red2 = CreatePen(PS_SOLID, 1, COLORREF(0x000000FF)); // Red pen
            SelectObject(hdc_mem, pen_red2);
            
            let x2 = rel(10.0/16.0, c);
            MoveToEx(hdc_mem, x2, y_top, None);
            LineTo(hdc_mem, x2, y_bottom);
            
            DeleteObject(pen_red2);
            
            // Step 5: Draw red dot at (10,13)
            let brush_red2 = CreateSolidBrush(COLORREF(0x000000FF)); // Red
            SelectObject(hdc_mem, brush_red2);
            SelectObject(hdc_mem, GetStockObject(NULL_PEN));
            
            let dot_x2 = rel(10.0/16.0, c);
            Ellipse(hdc_mem, dot_x2 - 1, dot_y - 1, dot_x2 + 2, dot_y + 2);
            
            DeleteObject(brush_red2);
            
            // Mark as opaque in mask
            let brush_mask_black = CreateSolidBrush(COLORREF(0x00000000));
            SelectObject(hdc_mask, brush_mask_black);
            Rectangle(hdc_mask,
                rel(9.0/16.0, c), rel(6.0/16.0, c),
                rel(13.0/16.0, c), rel(14.0/16.0, c)
            );
            DeleteObject(brush_mask_black);
        }
        
        SelectObject(hdc_mem, old_brush);
        SelectObject(hdc_mem, old_pen);
        DeleteObject(pen_outline);
        DeleteDC(hdc_mask);
        
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

