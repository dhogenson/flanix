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
    layout::{Constraint, Layout, Position, Rect},
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
    search_textbox: TextArea<'a>,
    searching: bool,
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
            search_textbox: TextArea::default(),
            searching: false,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(15),
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
            (false, KeyCode::Char('/')) => self.toggle_search(),
            (false, _) if self.searching => {
                self.search_textbox.input(key_event);
            }

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
        let count = self.visible_item_count();
        if count == 0 {
            self.selected.select(None);
            return;
        }

        let i = match self.selected.selected() {
            Some(i) if i < count - 1 => i + 1,
            _ => 0,
        };
        self.selected.select(Some(i));
    }

    fn previous(&mut self) {
        let count = self.visible_item_count();
        if count == 0 {
            self.selected.select(None);
            return;
        }

        let i = match self.selected.selected() {
            Some(i) if i > 0 && i < count => i - 1,
            _ => count - 1,
        };
        self.selected.select(Some(i));
    }

    fn search_query(&self) -> String {
        self.search_textbox.lines().join("").to_lowercase()
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let query = self.search_query();
        self.items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.to_lowercase().contains(&query).then_some(index))
            .collect()
    }

    fn visible_item_count(&self) -> usize {
        if self.searching {
            self.filtered_indices().len()
        } else {
            self.items.len()
        }
    }

    fn selected_item_index(&self) -> Option<usize> {
        let selected = self.selected.selected()?;
        if self.searching {
            self.filtered_indices().get(selected).copied()
        } else {
            Some(selected)
        }
    }

    fn current_selection(&self) -> Option<ConfigSelection> {
        self.selected_item_index()
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

    fn toggle_search(&mut self) {
        if self.searching {
            self.selected.select(self.selected_item_index());
            self.searching = false;
            return;
        }

        let selected = self.selected.selected();
        self.searching = true;
        let visible = self.filtered_indices();
        let index = selected
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .or_else(|| (!visible.is_empty()).then_some(0));
        self.selected.select(index);
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
        if self.editing {
            let title = Line::from(
                self.current_selection()
                    .map(|selection| format!(" Config - Editing \"{}\" ", selection.label()))
                    .unwrap_or_else(|| " Config - Editing ".to_string())
                    .bold(),
            );
            let instructions = Line::from(vec![
                " Save & Exit".into(),
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
            let title = Line::from(" Config ".bold());
            let instructions = Line::from(vec![
                " Save & Exit ".into(),
                "<Crtl C> ".bold().blue(),
                "Exit ".into(),
                "<F1> ".bold().blue(),
                "Edit selected ".into(),
                "<Enter> ".bold().blue(),
                "Toggle search ".into(),
                "</> ".bold().blue(),
            ]);
            let block = Block::bordered()
                .title(title.centered())
                .title_bottom(instructions)
                .border_set(border::THICK)
                .border_type(BorderType::Rounded);
            let inner = block.inner(area);
            block.render(area, buf);

            if self.searching {
                let [search_bar, body] =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(inner);
                let search_block = Block::bordered()
                    .title(" Search ")
                    .border_type(BorderType::Rounded);
                let search_inner = search_block.inner(search_bar);
                search_block.render(search_bar, buf);
                self.search_textbox.render(search_inner, buf);

                let items: Vec<ListItem> = self
                    .filtered_indices()
                    .into_iter()
                    .map(|index| ListItem::new(self.items[index].as_str()))
                    .collect();

                let list = List::new(items)
                    .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
                    .highlight_symbol("> ");

                StatefulWidget::render(list, body, buf, &mut self.selected);
            } else {
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
    fn search_filter_is_case_insensitive() -> Result<()> {
        let mut app = App::new()?;
        app.searching = true;
        app.search_textbox.insert_str("aws");

        assert_eq!(app.filtered_indices(), vec![2, 3, 4, 5]);

        Ok(())
    }

    #[test]
    fn search_selection_uses_filtered_index() -> Result<()> {
        let mut app = App::new()?;
        app.searching = true;
        app.search_textbox.insert_str("bucket");

        assert!(matches!(
            app.current_selection(),
            Some(ConfigSelection::BucketName)
        ));

        Ok(())
    }

    #[test]
    fn search_navigation_uses_filtered_count() -> Result<()> {
        let mut app = App::new()?;
        app.searching = true;
        app.search_textbox.insert_str("aws");
        app.selected.select(Some(3));

        app.next();
        assert_eq!(app.selected.selected(), Some(0));

        app.previous();
        assert_eq!(app.selected.selected(), Some(3));

        Ok(())
    }

    #[test]
    fn leaving_search_preserves_selected_item() -> Result<()> {
        let mut app = App::new()?;
        app.searching = true;
        app.search_textbox.insert_str("bucket");

        app.toggle_search();

        assert!(matches!(
            app.current_selection(),
            Some(ConfigSelection::BucketName)
        ));

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
