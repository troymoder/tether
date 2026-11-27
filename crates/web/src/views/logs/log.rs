use std::collections::{HashMap, VecDeque};

use super::line::{Level, Line, device_line, emulator_line};

/// The number of lines of a log the page keeps and asks the device for at
/// the start.
pub(super) const KEPT: usize = 20_000;

pub(super) struct Filters<'a> {
    /// Text a line must contain, in lower case.
    pub(super) text: String,
    pub(super) least: Level,
    pub(super) app: Option<String>,
    pub(super) names: &'a HashMap<u32, String>,
}

impl Filters<'_> {
    /// An emulator's own output belongs to no app, so the app filter is
    /// ignored for it.
    pub(super) fn pass(&self, line: &Line, emulator: bool) -> bool {
        // A line without a level is a note from logcat itself.
        let grave = line
            .level
            .map_or(self.least == Level::Verbose, |level| level >= self.least);
        let has = |part: &str| part.to_lowercase().contains(&self.text);
        // An app's processes are named after its package, optionally
        // followed by a colon and a suffix.
        let of_app = emulator
            || self.app.as_deref().is_none_or(|app| {
                line.pid
                    .and_then(|pid| self.names.get(&pid))
                    .is_some_and(|name| {
                        name.strip_prefix(app)
                            .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
                    })
            });

        grave && of_app && (self.text.is_empty() || has(line.message) || line.tag.is_some_and(has))
    }
}

#[derive(Default)]
pub(super) struct Log {
    /// True for an emulator's own output, whose lines have another form.
    pub(super) emulator: bool,
    /// The kept lines, oldest first.
    pub(super) lines: VecDeque<String>,
    /// The number of lines before the first kept one. A line is numbered
    /// from the first that ever came, so its number stays as older ones go.
    pub(super) before: u64,
    /// The numbers of the lines that pass the filters, in order.
    pub(super) shown: VecDeque<u64>,
    /// Bumped whenever `shown` is rebuilt, so the layout in frame.rs knows
    /// to start over.
    pub(super) sifted: u32,
}

impl Log {
    pub(super) fn parse<'a>(&self, raw: &'a str) -> Line<'a> {
        if self.emulator {
            emulator_line(raw)
        } else {
            device_line(raw)
        }
    }

    pub(super) fn line(&self, number: u64) -> Option<&str> {
        let at = usize::try_from(number.checked_sub(self.before)?).ok()?;

        self.lines.get(at).map(String::as_str)
    }

    pub(super) fn clear(&mut self) {
        self.lines.clear();
        self.shown.clear();
        self.before = 0;
        self.sifted = self.sifted.wrapping_add(1);
    }

    pub(super) fn filter(&mut self, filters: &Filters) {
        let passing = (self.before..)
            .zip(&self.lines)
            .filter(|(_, raw)| filters.pass(&self.parse(raw), self.emulator))
            .map(|(number, _)| number)
            .collect();

        self.shown = passing;
        self.sifted = self.sifted.wrapping_add(1);
    }

    /// Adds new lines. Without filters they are kept but not shown, which is
    /// how a paused log stays still. Returns how many shown lines were
    /// dropped because the oldest go past `KEPT`.
    pub(super) fn push(&mut self, new: &str, filters: Option<&Filters>) -> usize {
        for raw in new.lines() {
            let number = self.before + self.lines.len() as u64;

            if filters.is_some_and(|filters| filters.pass(&self.parse(raw), self.emulator)) {
                self.shown.push_back(number);
            }

            self.lines.push_back(raw.to_owned());
        }

        let excess = self.lines.len().saturating_sub(KEPT);

        self.lines.drain(..excess);
        self.before += excess as u64;

        let gone = self
            .shown
            .iter()
            .take_while(|number| **number < self.before)
            .count();

        self.shown.drain(..gone);

        gone
    }
}
