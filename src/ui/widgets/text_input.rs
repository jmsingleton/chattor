use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders};
use ratatui::Frame;
use tui_textarea::TextArea;

use crate::ui::theme::Theme;

/// Text input backed by tui-textarea: proper cursor rendering, unicode-width
/// handling, horizontal viewport scrolling for long input (e.g. 32-word
/// friend codes), and emacs-style editing keys for free.
pub struct TextInput {
    textarea: TextArea<'static>,
    single_line: bool,
}

impl TextInput {
    pub fn single_line(placeholder: &str) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_placeholder_text(placeholder);
        // The default cursor-line underline reads as a misrender in a
        // one-line input box.
        textarea.set_cursor_line_style(Style::default());
        Self {
            textarea,
            single_line: true,
        }
    }

    pub fn with_text(mut self, text: &str) -> Self {
        self.textarea.insert_str(text);
        self
    }

    /// Feed a key event to the editor. Returns false for keys the caller
    /// must handle itself: Esc always, and Enter in single-line mode.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => false,
            KeyCode::Enter if self.single_line => false,
            _ => {
                self.textarea.input(key);
                true
            }
        }
    }

    pub fn text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.textarea.lines().iter().all(|l| l.is_empty())
    }

    /// Render with a rounded border. `&mut self` because tui-textarea
    /// stores block/style on the widget itself.
    pub fn render(&mut self, f: &mut Frame, area: Rect, focused: bool, theme: &Theme) {
        let border = if focused {
            theme.border_focused
        } else {
            theme.border
        };
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border)),
        );
        self.textarea.set_style(Style::default().fg(theme.input_fg));
        self.textarea
            .set_placeholder_style(Style::default().fg(theme.input_placeholder));
        f.render_widget(&self.textarea, area);
    }
}

impl Clone for TextInput {
    fn clone(&self) -> Self {
        Self {
            textarea: self.textarea.clone(),
            single_line: self.single_line,
        }
    }
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("text", &self.text())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_appends_text() {
        let mut input = TextInput::single_line("hint");
        assert!(input.handle_key(key(KeyCode::Char('h'))));
        assert!(input.handle_key(key(KeyCode::Char('i'))));
        assert_eq!(input.text(), "hi");
        assert!(!input.is_empty());
    }

    #[test]
    fn enter_not_consumed_in_single_line() {
        let mut input = TextInput::single_line("");
        assert!(!input.handle_key(key(KeyCode::Enter)));
        assert_eq!(input.text(), ""); // no newline inserted
    }

    #[test]
    fn esc_not_consumed() {
        let mut input = TextInput::single_line("");
        assert!(!input.handle_key(key(KeyCode::Esc)));
    }

    #[test]
    fn backspace_and_cursor_movement() {
        let mut input = TextInput::single_line("").with_text("abc");
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.text(), "ab");
        input.handle_key(key(KeyCode::Left));
        input.handle_key(key(KeyCode::Char('x')));
        assert_eq!(input.text(), "axb");
    }

    #[test]
    fn unicode_input_no_panic() {
        let mut input = TextInput::single_line("");
        input.handle_key(key(KeyCode::Char('🦀')));
        input.handle_key(key(KeyCode::Char('字')));
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.text(), "🦀");
    }

    #[test]
    fn with_text_starts_with_cursor_at_end() {
        let mut input = TextInput::single_line("").with_text("ab");
        input.handle_key(key(KeyCode::Char('c')));
        assert_eq!(input.text(), "abc");
    }
}
