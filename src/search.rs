use std::ops::Range;

use crate::Session;

pub struct Search {
    original: Vec<Session>,
    substr: String,
    found: Vec<(Session, Range<usize>)>,
}

#[derive(Clone, Copy)]
pub enum Filtered<'a> {
    Original(&'a [Session]),
    Subset(&'a [(Session, Range<usize>)]),
}

impl<'a> Filtered<'a> {
    pub fn is_empty(self) -> bool {
        match self {
            Filtered::Original(sessions) => sessions.is_empty(),
            Filtered::Subset(items) => items.is_empty(),
        }
    }

    pub fn original(self) -> Option<&'a [Session]> {
        match self {
            Filtered::Original(sessions) => Some(sessions),
            Filtered::Subset(_) => None,
        }
    }

    pub fn subset(self) -> Option<&'a [(Session, Range<usize>)]> {
        match self {
            Filtered::Original(_) => None,
            Filtered::Subset(items) => Some(items),
        }
    }
}

impl Search {
    const LIMIT: usize = 128;

    pub fn is_empty(&self) -> bool {
        self.substr.is_empty()
    }

    pub fn new(original: &[Session]) -> Self {
        Self {
            original: original.to_owned(),
            substr: String::with_capacity(Self::LIMIT),
            found: Vec::default(),
        }
    }

    pub fn clear(&mut self) {
        self.substr.clear();
        self.found = self.find();
    }

    pub fn push(&mut self, ch: char) {
        if self.substr.len() < Self::LIMIT && ch.is_ascii() {
            self.substr.push(ch);
            self.found = self.find();
        }
    }

    pub fn pop(&mut self) {
        let _ = self.substr.pop();
        self.found = self.find();
    }

    pub fn as_str(&self) -> &str {
        &self.substr
    }

    pub fn sessions(&self) -> Filtered<'_> {
        if self.substr.is_empty() {
            Filtered::Original(&self.original)
        } else {
            Filtered::Subset(&self.found)
        }
    }

    fn find(&self) -> Vec<(Session, Range<usize>)> {
        self.original
            .iter()
            .filter_map(|session| {
                session
                    .name
                    .find_substr(&self.substr)
                    .map(|range| (session.clone(), range))
            })
            .collect()
    }
}
