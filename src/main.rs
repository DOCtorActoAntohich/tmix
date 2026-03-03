use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Sessions,
    New,
}

enum Action {
    Attach(String),
    New(String),
    Quit,
}

struct App {
    sessions: Vec<String>,
    list_state: ListState,
    focus: Panel,
    cwd: String,
}

impl App {
    fn new() -> Self {
        let sessions = list_sessions();
        let cwd = env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| String::from("?"));

        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }

        let focus = if sessions.is_empty() {
            Panel::New
        } else {
            Panel::Sessions
        };

        Self {
            sessions,
            list_state,
            focus,
            cwd,
        }
    }

    fn selected_session(&self) -> Option<&str> {
        self.list_state
            .selected()
            .and_then(|i| self.sessions.get(i))
            .map(|s| s.as_str())
    }

    fn move_up(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
    }

    fn move_down(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = self.list_state.selected().unwrap_or(0);
        let next = (i + 1).min(self.sessions.len() - 1);
        self.list_state.select(Some(next));
    }

    fn handle_key(&mut self, key: ratatui::crossterm::event::KeyEvent) -> Option<Action> {
        if key.code == KeyCode::Char('q')
            || key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Some(Action::Quit);
        }

        match key.code {
            KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                self.focus = match self.focus {
                    Panel::Sessions => Panel::New,
                    Panel::New => {
                        if self.sessions.is_empty() {
                            Panel::New
                        } else {
                            Panel::Sessions
                        }
                    }
                };
            }
            KeyCode::Up => {
                if self.focus == Panel::Sessions {
                    self.move_up();
                }
            }
            KeyCode::Down => {
                if self.focus == Panel::Sessions {
                    self.move_down();
                }
            }
            KeyCode::Enter => {
                return Some(match self.focus {
                    Panel::Sessions => match self.selected_session() {
                        Some(name) => Action::Attach(name.to_owned()),
                        None => Action::New(self.cwd.clone()),
                    },
                    Panel::New => Action::New(self.cwd.clone()),
                });
            }
            _ => {}
        }

        None
    }
}

fn list_sessions() -> Vec<String> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    }
}

fn run() -> io::Result<Action> {
    let mut terminal = ratatui::init();
    let mut app = App::new();

    let action = loop {
        terminal.draw(|frame| {
            let area = frame.area();

            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(area);

            // left panel: session list
            let left_border_style = if app.focus == Panel::Sessions {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let items: Vec<ListItem> = if app.sessions.is_empty() {
                vec![ListItem::new(Line::from(Span::styled(
                    "no sessions",
                    Style::default().fg(Color::DarkGray),
                )))]
            } else {
                app.sessions
                    .iter()
                    .map(|s| ListItem::new(Line::from(s.as_str())))
                    .collect()
            };

            let highlight_style = if app.focus == Panel::Sessions {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let highlight_symbol = if app.focus == Panel::Sessions { "> " } else { "  " };

            let list = List::new(items)
                .block(
                    Block::default()
                        .title(" sessions ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(left_border_style),
                )
                .highlight_style(highlight_style)
                .highlight_symbol(highlight_symbol);

            frame.render_stateful_widget(list, columns[0], &mut app.list_state);

            // right panel: new session
            let right_border_style = if app.focus == Panel::New {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let cwd_line = Line::from(vec![
                Span::styled("cwd  ", Style::default().fg(Color::DarkGray)),
                Span::styled(app.cwd.as_str(), Style::default().fg(Color::White)),
            ]);

            let hint = if app.focus == Panel::New {
                Line::from(Span::styled(
                    "press enter to create",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from("")
            };

            let paragraph = Paragraph::new(vec![cwd_line, Line::from(""), hint]).block(
                Block::default()
                    .title(" new session ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(right_border_style),
            );

            frame.render_widget(paragraph, columns[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            if let Some(action) = app.handle_key(key) {
                break action;
            }
        }
    };

    ratatui::restore();
    Ok(action)
}

fn main() -> io::Result<()> {
    match run()? {
        Action::Quit => Ok(()),
        Action::Attach(name) => Err(Command::new("tmux")
            .args(["attach-session", "-t", &name])
            .exec()),
        Action::New(cwd) => Err(Command::new("tmux")
            .args(["new-session", "-c", &cwd])
            .exec()),
    }
}
