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
use std::io::stdout;

pub struct App {
    items: Vec<String>,
    selected: ListState,
    exit: bool,
    viewport_origin: Option<Position>,
}

impl App {
    pub fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            items: vec!["Option 1".into(), "Option 2".into(), "Option 3".into()],
            selected: state,
            exit: false,
            viewport_origin: None,
        }
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
                self.handle_key_events(key_event);
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.exit();
            }
            KeyCode::Down => self.next(),
            KeyCode::Up => self.previous(),
            _ => {}
        }
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

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" This is a app ".bold());
        let block = Block::bordered()
            .title(title.centered())
            .border_set(border::THICK)
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|s| ListItem::new(s.as_str()))
            .collect();

        block.render(area, buf);

        let list = List::new(items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        StatefulWidget::render(list, inner, buf, &mut self.selected);
    }
}
