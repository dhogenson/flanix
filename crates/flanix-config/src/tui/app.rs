use super::config_selection::ConfigSelection;
use anyhow::Result;

use crossterm::{
    self,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style, Stylize},
    symbols::border,
    text::Line,
    widgets::{Block, BorderType, List, ListItem, ListState, StatefulWidget, Widget},
};
use ratatui_textarea::TextArea;
use std::io::stdout;

use crate::Config;

/// Main TUI struct for editing config
pub struct App<'a> {
    items: Vec<String>,
    selected: ListState,
    exit: bool,
    viewport_origin: Option<Position>,
    textarea: TextArea<'a>,
    editing: bool,
    new_config: Config,
}

impl<'a> App<'a> {
    pub fn new() -> Result<Self> {
        let mut state = ListState::default();
        state.select(Some(0));
        Ok(Self {
            items: ConfigSelection::all()
                .iter()
                .map(|s| s.label().into())
                .collect(),
            selected: state,
            exit: false,
            viewport_origin: None,
            textarea: TextArea::default(),
            editing: false,
            new_config: Config::new()?,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(10),
            },
        )?;

        crossterm::terminal::enable_raw_mode()?;
        self.app(&mut terminal)?;

        terminal.clear()?;
        if let Some(pos) = self.viewport_origin {
            terminal.set_cursor_position(pos)?;
        }
        crossterm::terminal::disable_raw_mode()?;

        Ok(())
    }

    fn app(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        self.viewport_origin = Some(frame.area().as_position());
        frame.render_widget(self, frame.area());
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_events(key_event)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        let ctrl = key_event.modifiers.contains(KeyModifiers::CONTROL);

        match (self.editing, key_event.code) {
            // Global
            (_, KeyCode::Char('c')) if ctrl => {
                self.save()?;
                self.exit();
            }

            // Navigating
            (false, KeyCode::Down) => self.next(),
            (false, KeyCode::Up) => self.previous(),
            (false, KeyCode::Enter) => self.start_editing(),
            (false, KeyCode::F(1)) => self.exit(),

            // Editing
            (true, KeyCode::F(1)) => self.editing = false,
            (true, KeyCode::Enter) => self.stop_editing(),
            (true, _) => {
                self.textarea.input(key_event);
            }

            _ => {}
        }
        Ok(())
    }

    fn next(&mut self) {
        let i = match self.selected.selected() {
            Some(i) if i + 1 < self.items.len() => i + 1,
            _ => 0, // wrap around
        };
        self.selected.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.selected.selected() {
            Some(0) | None => self.items.len() - 1,
            Some(i) => i - 1,
        };
        self.selected.select(Some(i));
    }

    fn current_selection(&self) -> Option<ConfigSelection> {
        self.selected
            .selected()
            .and_then(|i| ConfigSelection::from_u8(i as u8))
    }

    fn start_editing(&mut self) {
        if let Some(selection) = self.current_selection() {
            self.textarea.clear();
            self.textarea.insert_str(selection.value(&self.new_config));
            self.editing = true;
        }
    }

    fn stop_editing(&mut self) {
        if let Some(selection) = self.current_selection() {
            let value = self.textarea.lines().join("\n");
            selection.set_value(&mut self.new_config, &value);
        }
        self.editing = false;
    }

    fn save(&mut self) -> Result<()> {
        let config_path = Config::get_config_file()?;
        Config::write_config(&config_path, &self.new_config)?;
        Ok(())
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl<'a> Widget for &mut App<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" Config ".bold());

        if self.editing {
            let instructions = Line::from(vec![
                " Save and Exit".into(),
                " <Enter>".bold().blue(),
                " Exit".into(),
                " <F1> ".bold().blue(),
            ]);
            let block = Block::bordered()
                .title(title.centered())
                .title_bottom(instructions)
                .border_set(border::THICK)
                .border_type(BorderType::Rounded);
            let inner = block.inner(area);
            block.render(area, buf);
            self.textarea.render(inner, buf);
        } else {
            let instructions = Line::from(vec![
                " Save and Exit ".into(),
                "<Crtl C> ".bold().blue(),
                "Exit ".into(),
                "<F1> ".bold().blue(),
                "Edit selected ".into(),
                "<Enter> ".bold().blue(),
                "Up ".into(),
                "<UP> ".bold().blue(),
                "Down ".into(),
                "<DOWN> ".bold().blue(),
            ]);
            let block = Block::bordered()
                .title(title.centered())
                .title_bottom(instructions)
                .border_set(border::THICK)
                .border_type(BorderType::Rounded);
            let inner = block.inner(area);
            block.render(area, buf);
            let items: Vec<ListItem> = self
                .items
                .iter()
                .map(|s| ListItem::new(s.as_str()))
                .collect();

            let list = List::new(items)
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
                .highlight_symbol("> ");
            StatefulWidget::render(list, inner, buf, &mut self.selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn next_increments() -> Result<()> {
        let mut app = App::new()?;
        app.next();
        assert_eq!(app.selected.selected(), Some(1));

        Ok(())
    }

    #[test]
    fn next_wraps_while_at_max() -> Result<()> {
        let mut app = App::new()?;
        app.selected.select(Some(app.items.len() - 1));
        app.next();
        assert_eq!(app.selected.selected(), Some(0));

        Ok(())
    }

    #[test]
    fn previous_wraps_while_at_zero() -> Result<()> {
        let mut app = App::new()?;
        app.previous();
        assert_eq!(app.selected.selected(), Some(app.items.len() - 1));

        Ok(())
    }

    #[test]
    fn previous_decrement() -> Result<()> {
        let mut app = App::new()?;
        app.selected.select(Some(1));
        app.previous();
        assert_eq!(app.selected.selected(), Some(0));

        Ok(())
    }

    #[test]
    fn exit() -> Result<()> {
        let mut app = App::new()?;
        app.exit();
        assert!(app.exit);

        Ok(())
    }

    #[test]
    fn ctrl_c_key_event() -> Result<()> {
        let mut app = App::new()?;
        app.handle_key_events(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))?;
        assert!(app.exit);

        Ok(())
    }

    #[test]
    fn start_editing() -> Result<()> {
        let mut app = App::new()?;
        app.start_editing();
        assert!(app.editing);
        Ok(())
    }

    #[test]
    fn down_key_event() -> Result<()> {
        let mut app = App::new()?;
        app.handle_key_events(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))?;
        assert_eq!(app.selected.selected(), Some(1));

        Ok(())
    }

    #[test]
    fn down_key_event_at_max() -> Result<()> {
        let mut app = App::new()?;
        app.selected.select(Some(app.items.len() - 1));
        app.handle_key_events(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))?;
        assert_eq!(app.selected.selected(), Some(0));

        Ok(())
    }

    #[test]
    fn up_key_event() -> Result<()> {
        let mut app = App::new()?;
        app.handle_key_events(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))?;
        assert_eq!(app.selected.selected(), Some(app.items.len() - 1));

        Ok(())
    }

    #[test]
    fn up_key_event_at_max() -> Result<()> {
        let mut app = App::new()?;
        app.selected.select(Some(1));
        app.handle_key_events(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))?;
        assert_eq!(app.selected.selected(), Some(0));

        Ok(())
    }

    #[test]
    fn enter_key_event() -> Result<()> {
        let mut app = App::new()?;
        app.handle_key_events(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
        assert!(app.editing);

        Ok(())
    }
}
