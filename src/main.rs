use std::num::NonZero;
use std::ops::Range;
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::Context;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};
use tmix::{Search, Session, Tmux};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Sessions,
    New,
}

enum Action {
    Attach(Session),
    New(Session),
    Quit,
}

struct App {
    tmux: Tmux,
    sessions_table_state: TableState,
    focus: Panel,
    search: Search,
}

impl App {
    fn new(tmux: Tmux) -> Self {
        let mut sessions_table_state = TableState::default();
        if !tmux.sessions.is_empty() {
            sessions_table_state.select(Some(0));
        }

        let focus = if tmux.sessions.is_empty() {
            Panel::New
        } else {
            Panel::Sessions
        };

        let search = Search::new(&tmux.sessions);

        Self {
            tmux,
            sessions_table_state,
            focus,
            search,
        }
    }

    fn selected_session(&self) -> Option<&Session> {
        self.sessions_table_state
            .selected()
            .and_then(|i| match self.search.sessions() {
                tmix::Filtered::Original(sessions) => sessions.get(i),
                tmix::Filtered::Subset(items) => items.get(i).map(|(s, _)| s),
            })
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let [main_area, help_bar_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .areas::<2>(frame.area());

        let [session_list_area, new_session_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas::<2>(main_area);

        let [sessions_area, search_item_area] = if self.search.is_empty() {
            [session_list_area, Rect::default()]
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .areas::<2>(session_list_area)
        };

        self.draw_sessions_list(frame, sessions_area);
        self.draw_new_session_window(frame, new_session_area);
        self.draw_help_bar(frame, help_bar_area);
        self.draw_search_item(frame, search_item_area);
    }

    fn draw_search_item(&self, frame: &mut ratatui::Frame, area: Rect) {
        let mut line = Line::default();
        line.push_span("Searching: ");
        line.push_span(self.search.as_str());

        frame.render_widget(line, area);
    }

    fn draw_help_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let bindings: &[(&str, &str)] = &[
            ("Esc", "Quit"),
            ("↑↓", "Navigate"),
            ("Tab / ←→", "Switch panel"),
            ("Enter", "Confirm"),
        ];

        let spans: Vec<Span> = bindings
            .iter()
            .flat_map(|(key, description)| {
                [
                    Span::from(format!(" {key} "))
                        .style(Style::default().fg(Color::Black).bg(Color::DarkGray)),
                    Span::from(format!("  {description}  "))
                        .style(Style::default().fg(Color::DarkGray)),
                ]
            })
            .collect();

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn draw_sessions_list(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let active = self.focus == Panel::Sessions;

        let border_style = if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let table_widths = [
            Constraint::Percentage(40),
            Constraint::Percentage(60),
            Constraint::Length(2),
        ];

        let sessions: Vec<(&Session, Option<Range<usize>>)> = match self.search.sessions() {
            any if any.is_empty() => Vec::new(),
            tmix::Filtered::Original(sessions) => sessions.iter().map(|s| (s, None)).collect(),
            tmix::Filtered::Subset(items) => {
                items.iter().map(|(s, r)| (s, Some(r.clone()))).collect()
            }
        };

        let table = if sessions.is_empty() {
            let rows = [Row::new([Cell::new("<no sessions>")])];
            let widths = [Constraint::Min(1)];
            Table::new(rows, widths)
        } else {
            let rows = sessions
                .into_iter()
                .enumerate()
                .map(|(ith, (session, range))| {
                    let Session { name, path } = session;
                    let is_highlighted = Some(ith) == self.sessions_table_state.selected();
                    let fg_color = if is_highlighted {
                        Color::Black
                    } else {
                        Color::White
                    };
                    let name = match range {
                        Some(range) => Line::from(vec![
                            Span::raw(&name.as_str()[..range.start])
                                .style(Style::default().fg(fg_color)),
                            Span::raw(&name.as_str()[range.clone()])
                                .style(Style::default().fg(Color::Yellow)),
                            Span::raw(&name.as_str()[range.end..])
                                .style(Style::default().fg(fg_color)),
                        ]),
                        None => Line::from(name.as_str()).style(Style::default().fg(fg_color)),
                    };
                    let path = Text::from(path.to_string_lossy())
                        .alignment(HorizontalAlignment::Right)
                        .style(Style::default().fg(fg_color));
                    [Cell::new(name), Cell::new(path)]
                })
                .map(Row::new);

            Table::new(rows, table_widths)
        };

        let highlight_style = if active {
            Style::default()
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let highlight_symbol =
            Text::raw(if active { "> " } else { "  " }).style(Style::default().fg(Color::Black));

        let table = table
            .block(
                Block::default()
                    .title(" Attach to an existing session ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style),
            )
            .row_highlight_style(highlight_style)
            .highlight_symbol(highlight_symbol);

        frame.render_stateful_widget(table, area, &mut self.sessions_table_state);
    }

    fn draw_new_session_window(&self, frame: &mut ratatui::Frame, area: Rect) {
        let active = self.focus == Panel::New;

        let border_style = if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let cwd_line = Line::from(vec![
            Span::styled("At path:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.tmux.cwd_session.path.display().to_string(),
                Style::default().fg(Color::Green),
            ),
        ]);

        let name_line = Line::from(vec![
            Span::styled("Will be named: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.tmux.cwd_session.name.as_str(),
                Style::default().fg(Color::Yellow),
            ),
        ]);

        let hint = if active {
            Line::from(Span::styled(
                "Press Enter to create",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from("")
        };

        let paragraph = Paragraph::new(vec![cwd_line, name_line, Line::from(""), hint])
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" Start new session ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style),
            );

        frame.render_widget(paragraph, area);
    }

    fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Some(Action::Quit);
            }
            KeyCode::Esc if self.search.is_empty() => return Some(Action::Quit),
            KeyCode::Esc => {
                self.focus = Panel::Sessions;
                self.search.clear();
            }
            KeyCode::Char(ch) if !self.tmux.sessions.is_empty() => {
                self.focus = Panel::Sessions;
                self.search.push(ch);
                self.reselect();
            }
            KeyCode::Backspace => {
                self.focus = Panel::Sessions;
                self.search.pop();
                self.reselect();
            }
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                self.focus = match self.focus {
                    Panel::Sessions => Panel::New,
                    Panel::New => {
                        if self.tmux.sessions.is_empty() {
                            Panel::New
                        } else {
                            Panel::Sessions
                        }
                    }
                };
            }
            KeyCode::Up if self.focus == Panel::Sessions => {
                self.sessions_table_state.select_previous();
                self.reselect();
            }
            KeyCode::Down if self.focus == Panel::Sessions => {
                self.sessions_table_state.select_next();
                self.reselect();
            }
            KeyCode::Enter => match self.focus {
                Panel::New => return Some(Action::New(self.tmux.cwd_session.clone())),
                Panel::Sessions if !self.search.sessions().is_empty() => {
                    match self.selected_session() {
                        Some(session) => return Some(Action::Attach(session.to_owned())),
                        None => return Some(Action::New(self.tmux.cwd_session.clone())),
                    }
                }
                _ => {}
            },
            _ => {}
        }

        None
    }

    fn reselect(&mut self) {
        let to_select = self.sessions_table_state.selected().map(|current| {
            self.search
                .sessions()
                .subset()
                .map(|filtered| filtered.len())
                .and_then(NonZero::new)
                .map(|filtered| filtered.get().saturating_sub(1).min(current))
                .unwrap_or(current)
        });
        self.sessions_table_state.select(to_select);
    }

    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<Action> {
        loop {
            terminal
                .draw(|frame| self.draw(frame))
                .context("Failed to render the frame")?;

            let Event::Key(key) = event::read().context("Failed to receive event")? else {
                continue;
            };

            if let Some(action) = self.handle_key(key) {
                break Ok(action);
            };
        }
    }
}

fn run_tui(app: &mut App) -> anyhow::Result<Action> {
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new(Tmux::load()?);

    match run_tui(&mut app)? {
        Action::Quit => Ok(()),
        Action::Attach(Session { name, path: _ }) => Err(Command::new("tmux")
            .arg("attach-session")
            .arg("-t")
            .arg(name)
            .exec()
            .into()),
        Action::New(Session { name, path }) => {
            let mut cmd = Command::new("tmux");
            cmd.arg("new-session")
                .arg("-c")
                .arg(path)
                .arg("-s")
                .arg(name);
            Err(cmd.exec().into())
        }
    }
}
