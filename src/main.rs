use std::env;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
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
    New(PathBuf),
    Quit,
}

struct TmuxState {
    sessions: Vec<String>,
    cwd: PathBuf,
}

impl TmuxState {
    fn load() -> Result<Self> {
        let sessions = Self::list_sessions();
        let cwd = env::current_dir().context("failed to get current directory")?;
        Ok(Self { sessions, cwd })
    }

    fn list_sessions() -> Vec<String> {
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", "#{session_name}"])
            .output();

        match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            _ => vec![],
        }
    }
}

struct App {
    tmux: TmuxState,
    list_state: ListState,
    focus: Panel,
}

impl App {
    fn new(tmux: TmuxState) -> Self {
        let mut list_state = ListState::default();
        if !tmux.sessions.is_empty() {
            list_state.select(Some(0));
        }

        let focus = if tmux.sessions.is_empty() {
            Panel::New
        } else {
            Panel::Sessions
        };

        Self {
            tmux,
            list_state,
            focus,
        }
    }

    fn selected_session(&self) -> Option<&str> {
        self.list_state
            .selected()
            .and_then(|i| self.tmux.sessions.get(i))
            .map(|s| s.as_str())
    }

    fn draw(&mut self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        let left_border_style = if self.focus == Panel::Sessions {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let items: Vec<ListItem> = if self.tmux.sessions.is_empty() {
            vec![ListItem::new(Line::from(Span::styled(
                "no sessions",
                Style::default().fg(Color::DarkGray),
            )))]
        } else {
            self.tmux
                .sessions
                .iter()
                .map(|s| ListItem::new(Line::from(s.as_str())))
                .collect()
        };

        let highlight_style = if self.focus == Panel::Sessions {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let highlight_symbol = if self.focus == Panel::Sessions {
            "> "
        } else {
            "  "
        };

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

        frame.render_stateful_widget(list, columns[0], &mut self.list_state);

        // right panel: new session
        let right_border_style = if self.focus == Panel::New {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let cwd_line = Line::from(vec![
            Span::styled("cwd  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.tmux.cwd.display().to_string(),
                Style::default().fg(Color::White),
            ),
        ]);

        let hint = if self.focus == Panel::New {
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
                        if self.tmux.sessions.is_empty() {
                            Panel::New
                        } else {
                            Panel::Sessions
                        }
                    }
                };
            }
            KeyCode::Up if self.focus == Panel::Sessions => {
                self.list_state.select_previous();
            }
            KeyCode::Down if self.focus == Panel::Sessions => {
                self.list_state.select_next();
            }
            KeyCode::Enter => {
                let action = match self.focus {
                    Panel::Sessions => match self.selected_session() {
                        Some(name) => Action::Attach(name.to_owned()),
                        None => Action::New(self.tmux.cwd.clone()),
                    },
                    Panel::New => Action::New(self.tmux.cwd.clone()),
                };
                return Some(action);
            }
            _ => {}
        }

        None
    }
}

fn run_tui(app: &mut App) -> Result<Action> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<Action> {
    loop {
        terminal.draw(|frame| app.draw(frame))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };

        let Some(action) = app.handle_key(key) else {
            continue;
        };

        return Ok(action);
    }
}

fn main() -> Result<()> {
    let tmux = TmuxState::load()?;
    let mut app = App::new(tmux);

    match run_tui(&mut app)? {
        Action::Quit => Ok(()),
        Action::Attach(name) => Err(Command::new("tmux")
            .arg("attach-session")
            .arg("-t")
            .arg(name)
            .exec()
            .into()),
        Action::New(cwd) => Err(Command::new("tmux")
            .arg("new-session")
            .arg("-c")
            .arg(cwd)
            .exec()
            .into()),
    }
}
