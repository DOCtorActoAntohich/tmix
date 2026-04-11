use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};
use tmix::{Session, Tmux};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Sessions,
    New,
}

enum Action {
    Attach(Session),
    New(PathBuf),
    Quit,
}

struct App {
    tmux: Tmux,
    sessions_table_state: TableState,
    focus: Panel,
}

impl App {
    fn new(tmux: Tmux) -> Self {
        let mut table_state = TableState::default();
        if !tmux.sessions.is_empty() {
            table_state.select(Some(0));
        }

        let focus = if tmux.sessions.is_empty() {
            Panel::New
        } else {
            Panel::Sessions
        };

        Self {
            tmux,
            sessions_table_state: table_state,
            focus,
        }
    }

    fn selected_session(&self) -> Option<&Session> {
        self.sessions_table_state
            .selected()
            .and_then(|i| self.tmux.sessions.get(i))
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

        self.draw_sessions_list(frame, session_list_area);
        self.draw_new_session_window(frame, new_session_area);
        self.draw_help_bar(frame, help_bar_area);
    }

    fn draw_help_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let bindings: &[(&str, &str)] = &[
            ("Q / Esc", "Quit"),
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

        let table = if self.tmux.sessions.is_empty() {
            let rows = [Row::new([Cell::new("<no sessions>")])];
            let widths = [Constraint::Min(1)];
            Table::new(rows, widths)
        } else {
            let rows: Vec<_> = self
                .tmux
                .sessions
                .iter()
                .map(|Session { name, path }| {
                    [
                        Cell::new(name.as_str()),
                        Cell::new(
                            Text::from(path.to_string_lossy())
                                .alignment(HorizontalAlignment::Right),
                        ),
                    ]
                })
                .map(Row::new)
                .collect();
            let widths = [
                Constraint::Percentage(40),
                Constraint::Percentage(60),
                Constraint::Length(2),
            ];
            Table::new(rows, widths)
        };

        let highlight_style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let table = table
            .block(
                Block::default()
                    .title(" Attach to an existing session ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(border_style),
            )
            .row_highlight_style(highlight_style)
            .highlight_symbol(if active { "> " } else { "  " });

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
                self.tmux.cwd.display().to_string(),
                Style::default().fg(Color::Green),
            ),
        ]);

        let name_line = Line::from(vec![
            Span::styled("Will be named: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                self.tmux
                    .cwd
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| String::from("(default by tmux)")),
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
                self.sessions_table_state.select_previous();
            }
            KeyCode::Down if self.focus == Panel::Sessions => {
                self.sessions_table_state.select_next();
            }
            KeyCode::Enter => {
                let action = match self.focus {
                    Panel::Sessions => match self.selected_session() {
                        Some(session) => Action::Attach(session.to_owned()),
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
        Action::New(cwd) => {
            let mut cmd = Command::new("tmux");
            cmd.arg("new-session").arg("-c").arg(&cwd);
            if let Some(name) = cwd.file_name() {
                cmd.arg("-s").arg(name);
            }
            Err(cmd.exec().into())
        }
    }
}
