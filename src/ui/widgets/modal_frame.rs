use ratatui::layout::Rect;

/// Compute a centered modal area sized `pct_x`/`pct_y` percent of `screen`,
/// but never smaller than `min_w`×`min_h` cells. If the screen itself can't
/// fit the minimum, the modal takes the whole screen instead of collapsing.
pub fn modal_area(screen: Rect, pct_x: u16, pct_y: u16, min_w: u16, min_h: u16) -> Rect {
    let w = (((screen.width as u32) * (pct_x as u32)) / 100) as u16;
    let h = (((screen.height as u32) * (pct_y as u32)) / 100) as u16;
    let w = w.max(min_w).min(screen.width);
    let h = h.max(min_h).min(screen.height);
    Rect {
        x: screen.x + (screen.width - w) / 2,
        y: screen.y + (screen.height - h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_sizing_on_large_screen() {
        let area = modal_area(Rect::new(0, 0, 100, 40), 60, 50, 40, 10);
        assert_eq!(area.width, 60);
        assert_eq!(area.height, 20);
        assert_eq!(area.x, 20);
        assert_eq!(area.y, 10);
    }

    #[test]
    fn clamps_up_to_minimum_on_small_screen() {
        // 60% of 70 = 42 < min 50 -> use 50
        let area = modal_area(Rect::new(0, 0, 70, 24), 60, 40, 50, 12);
        assert_eq!(area.width, 50);
        assert_eq!(area.height, 12);
    }

    #[test]
    fn tiny_screen_falls_back_to_full_screen() {
        let area = modal_area(Rect::new(0, 0, 30, 8), 60, 40, 50, 12);
        assert_eq!(area, Rect::new(0, 0, 30, 8));
    }
}
