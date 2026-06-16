use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;

use crate::ui::theme::Theme;

/// Render `lines` bottom-anchored in `area`, scrolled up by `scroll_offset`
/// display rows. Wrap-aware: long lines count for the rows they actually
/// occupy after wrapping (the old code skipped logical lines before wrap was
/// applied, clipping the newest messages).
///
/// Returns the clamped offset actually applied — callers must write it back
/// to their state so PageUp can't scroll past the top of history.
#[allow(dead_code)]
pub fn render_scrollable(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'_>>,
    scroll_offset: usize,
    theme: &Theme,
) -> usize {
    if area.width < 2 || area.height == 0 {
        return 0;
    }
    // Reserve the rightmost column as a scrollbar gutter so the bar never
    // overlaps text.
    let text_area = Rect {
        width: area.width - 1,
        ..area
    };

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let total = paragraph.line_count(text_area.width);
    let height = area.height as usize;
    let max_offset = total.saturating_sub(height);
    let offset = scroll_offset.min(max_offset);
    let scroll_y = (total.saturating_sub(height + offset)).min(u16::MAX as usize) as u16;

    f.render_widget(paragraph.scroll((scroll_y, 0)), text_area);

    if total > height {
        let mut sb_state = ScrollbarState::new(max_offset).position(max_offset - offset);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.fg_dim)),
            area,
            &mut sb_state,
        );
    }

    offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(width: u16, height: u16, lines: Vec<String>, offset: usize) -> (String, usize) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::preset("dark");
        let mut clamped = 0;
        terminal
            .draw(|f| {
                let ls: Vec<Line> = lines.iter().map(|s| Line::from(s.as_str())).collect();
                clamped = render_scrollable(f, f.size(), ls, offset, &theme);
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        (text, clamped)
    }

    #[test]
    fn bottom_line_visible_when_long_line_wraps() {
        // 100-char line wraps to ~6 rows at width 19 (20 minus gutter).
        // Old wrap-blind math would have clipped "LAST" off-screen.
        let (text, _) = draw(20, 5, vec!["x".repeat(100), "LAST".to_string()], 0);
        assert!(text.contains("LAST"));
    }

    #[test]
    fn offset_clamps_to_top_of_history() {
        let lines: Vec<String> = (0..10).map(|i| format!("line{}", i)).collect();
        let (text, clamped) = draw(20, 4, lines, 999);
        assert_eq!(clamped, 6); // 10 lines - 4 visible
        assert!(text.contains("line0")); // clamped to very top
    }

    #[test]
    fn no_scroll_needed_fits_entirely() {
        let (text, clamped) = draw(20, 10, vec!["a".to_string(), "b".to_string()], 0);
        assert_eq!(clamped, 0);
        assert!(text.contains('a') && text.contains('b'));
    }

    #[test]
    fn zero_size_area_is_safe() {
        let backend = TestBackend::new(5, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::preset("dark");
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 1, 0);
                assert_eq!(render_scrollable(f, area, vec![], 5, &theme), 0);
            })
            .unwrap();
    }
}
