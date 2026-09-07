use anyhow::Result;

use crossterm::{
    self,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style, Stylize},
    symbols::border,
    text::Line,
    widgets::{Block, BorderType, Widget},
};
use ratatui_textarea::TextArea;
use std::io::stdout;

pub struct App<'a> {
    inputs: Vec<TextArea<'a>>,
    current_input: u8,
    previous_input: u8,
    exit: bool,
    viewport_origin: Option<Position>,
}

impl<'a> App<'a> {
    pub fn new() -> Self {
        let mut inputs = Vec::new();
        inputs.push(TextArea::default());
        inputs.push(TextArea::default());
        for (i, input) in inputs.iter_mut().enumerate() {
            Self::apply_input_focus(input, i, i == 0);
        }
        Self {
            inputs,
            current_input: 0,
            previous_input: 0,
            exit: false,
            viewport_origin: None,
        }
    }

    fn input_block(i: usize, focused: bool) -> Block<'static> {
        Block::bordered()
            .title(Line::from(format!(" Input {} ", i + 1)).bold())
            .border_type(BorderType::Rounded)
            .border_style(if focused {
                Style::default().cyan()
            } else {
                Style::default()
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
        self.update_focus();
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }

        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        self.viewport_origin = Some(frame.area().as_position());
        frame.render_widget(&*self, frame.area());
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_events(key_event);
                self.update_focus();
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
            KeyCode::Down => self.increment_input(),
            KeyCode::Up => self.decrement_input(),
            _ => {
                self.inputs[self.current_input as usize].input(key_event);
            }
        }
    }

    fn decrement_input(&mut self) {
        if self.current_input > 0 {
            self.current_input -= 1;
        }
    }

    fn increment_input(&mut self) {
        if self.current_input < 1 {
            self.current_input += 1;
        }
    }

    fn update_focus(&mut self) {
        let old = self.previous_input as usize;
        let new = self.current_input as usize;
        Self::apply_input_focus(&mut self.inputs[old], old, false);
        Self::apply_input_focus(&mut self.inputs[new], new, true);
        self.previous_input = self.current_input;
    }

    fn apply_input_focus(input: &mut TextArea<'_>, index: usize, focused: bool) {
        input.set_block(Self::input_block(index, focused));
        if focused {
            input.set_cursor_line_style(Style::default().add_modifier(Modifier::UNDERLINED));
            input.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        } else {
            input.set_cursor_line_style(Style::default());
            input.set_cursor_style(Style::default());
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}

impl Widget for &App<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Line::from(" This is a app ".bold());
        let block = Block::bordered()
            .title(title.centered())
            .border_set(border::THICK)
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);
        block.render(area, buf);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(inner);
        self.inputs[0].render(chunks[0], buf);
        self.inputs[1].render(chunks[1], buf);
    }
}
