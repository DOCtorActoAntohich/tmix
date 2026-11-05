mod search;

pub use crate::search::{Filtered, Search};

use std::ffi::OsStr;
use std::ops::Range;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use nom::branch::alt;
use nom::bytes::tag;
use nom::character::complete::{line_ending, not_line_ending};
use nom::multi::many0;
use nom::sequence::terminated;
use nom::{IResult, Parser};

pub struct Tmux {
    pub cwd_session: Session,
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: SessionName,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionName(String);

impl Default for SessionName {
    fn default() -> Self {
        Self("default".to_owned())
    }
}

impl SessionName {
    pub fn new_normalized(s: impl AsRef<str>) -> Self {
        let name = s
            .as_ref()
            .chars()
            .map(|c| match c {
                '.' => '-',
                ':' => '-',
                other => other,
            })
            .collect();
        Self(name)
    }

    pub fn as_str(&self) -> &str {
        let Self(name) = self;
        name.as_str()
    }

    pub fn find_substr(&self, sub: &str) -> Option<Range<usize>> {
        self.as_str()
            .find(sub)
            .map(|start| start..start + sub.len())
    }
}

impl AsRef<OsStr> for SessionName {
    fn as_ref(&self) -> &OsStr {
        let Self(name) = self;
        name.as_ref()
    }
}

impl Tmux {
    pub fn load() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("Failed to get the current directory")?;
        let sessions = Self::list_sessions()?;

        let here_name = cwd
            .file_name()
            .map(OsStr::to_string_lossy)
            .map(SessionName::new_normalized)
            .unwrap_or_default();
        let cwd_session = Session {
            name: here_name,
            path: cwd,
        };

        Ok(Self {
            cwd_session,
            sessions,
        })
    }

    pub fn list_sessions() -> anyhow::Result<Vec<Session>> {
        const QUERY: &str = "#{session_name}\n#{session_path}";
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", QUERY])
            .output()
            .context("Failed to get command output")?;

        let output = if !output.stdout.is_empty() {
            output.stdout
        } else {
            output.stderr
        };

        let output = str::from_utf8(&output)
            .context("Tmux session output wasn't a printable UTF8 string")?;

        let (rest, sessions) =
            all_session_names(output).map_err(|_| anyhow::anyhow!("Failed to parse output"))?;
        if !rest.is_empty() {
            return Err(anyhow::anyhow!("Some command output wasn't parsed: {rest}"));
        }

        Ok(sessions)
    }
}

fn session_name(input: &str) -> IResult<&str, Session> {
    let (input, name) = terminated(not_line_ending, line_ending).parse(input)?;
    let (input, path) = terminated(not_line_ending, line_ending).parse(input)?;
    Ok((
        input,
        Session {
            name: SessionName::new_normalized(name),
            path: path.into(),
        },
    ))
}

fn many_session_names(input: &str) -> IResult<&str, Vec<Session>> {
    many0(session_name).parse(input)
}

/// Nothing to parse when there's the error message
fn dead_daemon_error_message(input: &str) -> IResult<&str, Vec<Session>> {
    let (_input, _tag) =
        alt((tag("no server running on "), tag("error connecting"))).parse(input)?;
    Ok(("", Vec::default()))
}

fn all_session_names(input: &str) -> IResult<&str, Vec<Session>> {
    alt((dead_daemon_error_message, many_session_names)).parse(input)
}
