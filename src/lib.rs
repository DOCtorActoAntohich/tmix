use std::{path::PathBuf, process::Command};

use anyhow::Context;
use nom::{
    IResult, Parser,
    character::complete::{line_ending, not_line_ending},
    multi::many0,
    sequence::terminated,
};

pub struct Tmux {
    pub cwd: PathBuf,
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub path: PathBuf,
}

impl Tmux {
    pub fn load() -> anyhow::Result<Self> {
        let sessions = Self::list_sessions()?;
        let cwd = std::env::current_dir().context("Failed to get the current directory")?;
        Ok(Self { sessions, cwd })
    }

    pub fn list_sessions() -> anyhow::Result<Vec<Session>> {
        const QUERY: &str = "#{session_name}\n#{session_path}";
        let output = Command::new("tmux")
            .args(["list-sessions", "-F", QUERY])
            .output()
            .context("Failed to get command output")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to list tmux sessions: {}",
                output.status
            ));
        }

        let output = str::from_utf8(&output.stdout)
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
            name: name.to_owned(),
            path: path.into(),
        },
    ))
}

fn all_session_names(input: &str) -> IResult<&str, Vec<Session>> {
    many0(session_name).parse(input)
}
