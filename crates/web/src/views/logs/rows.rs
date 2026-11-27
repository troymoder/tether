use unicode_width::UnicodeWidthChar;

use super::line::{Level, Line};
use super::log::Log;
use crate::ui::xterm;

/// The width of the tag column, in cells.
const TAG: usize = 22;

const MOST_ROWS: usize = 200;

/// The cells a tab takes, which is what a stack trace is indented by.
const TAB: usize = 4;

/// Read from the stylesheet, so they follow the theme.
#[derive(Clone, Copy)]
pub(super) struct Colours {
    text: u32,
    muted: u32,
    accent: u32,
    warn: u32,
    bad: u32,
    ground: u32,
    chosen: u32,
}

impl Colours {
    pub(super) fn now() -> Self {
        let token = |name: &str, otherwise: u32| xterm::token_number(name).unwrap_or(otherwise);

        Self {
            text: token("--text", 0xe6e6e6),
            muted: token("--muted", 0x9a9a9a),
            accent: token("--accent", 0x6ea8ff),
            warn: token("--warn", 0xe3b341),
            bad: token("--bad", 0xff6b63),
            ground: token("--raised", 0x0c0c0c),
            chosen: token("--chosen", 0x2a2a2a),
        }
    }
}

/// Mixes `hundredths` percent of `other` into `colour`, both as `0xRRGGBB`.
fn mix(colour: u32, other: u32, hundredths: u32) -> u32 {
    let part = |shift: u32| {
        (((colour >> shift & 0xff) * (100 - hundredths) + (other >> shift & 0xff) * hundredths)
            / 100)
            << shift
    };

    part(16) | part(8) | part(0)
}

/// A `0xRRGGBB` colour as the `R;G;B` of an escape sequence.
fn rgb(colour: u32) -> String {
    format!(
        "{};{};{}",
        colour >> 16 & 0xff,
        colour >> 8 & 0xff,
        colour & 0xff
    )
}

struct Row {
    told: String,
    /// The cells written so far.
    places: usize,
    columns: usize,
}

impl Row {
    fn new(ground: u32, columns: usize) -> Self {
        Self {
            told: format!("\u{1b}[0;48;2;{}m", rgb(ground)),
            places: 0,
            columns,
        }
    }

    fn colour(&mut self, colour: u32, bold: bool) {
        let weight = if bold { 1 } else { 22 };

        self.told
            .push_str(&format!("\u{1b}[{weight};38;2;{}m", rgb(colour)));
    }

    fn put(&mut self, text: &str, until: usize) {
        for (letter, places) in letters(text) {
            if self.places + places > until.min(self.columns) {
                break;
            }

            self.told.push(letter);
            self.places += places;
        }
    }

    fn pad(&mut self, to: usize) {
        while self.places < to.min(self.columns) {
            self.told.push(' ');
            self.places += 1;
        }
    }

    /// A full row skips the fill, since erasing to the end there would erase
    /// its last letter.
    fn finish(mut self) -> String {
        if self.places < self.columns {
            self.told.push_str("\u{1b}[K");
        }

        self.told
    }
}

/// The characters of some text with the cells each takes in a terminal: two
/// for a wide one and none for a combining one. A device's log is untrusted,
/// so a tab becomes spaces and any other control character one space, which
/// keeps the terminal from acting on it.
fn letters(text: &str) -> impl Iterator<Item = (char, usize)> + '_ {
    text.chars().flat_map(|letter| {
        let (letter, times) = match letter {
            '\t' => (' ', TAB),
            letter if letter.is_control() => (' ', 1),
            letter => (letter, 1),
        };

        std::iter::repeat_n((letter, letter.width().unwrap_or(0)), times)
    })
}

impl Log {
    /// The cells before the message, which is also where a wrapped message
    /// continues on the rows below.
    fn indent(line: &Line) -> usize {
        // An emulator's own output has no tag, and neither does a note.
        if line.tag.is_none() { 16 } else { 17 + TAG }
    }

    pub(super) fn rows_of(&self, number: u64, columns: usize, wrap: bool) -> usize {
        if !wrap {
            return 1;
        }

        let line = self.parse(self.line(number).unwrap_or_default());
        let room = columns.saturating_sub(Self::indent(&line)).max(1);
        let places: usize = letters(line.message).map(|(_, places)| places).sum();

        places.div_ceil(room).clamp(1, MOST_ROWS)
    }

    pub(super) fn rows(
        &self,
        number: u64,
        (columns, wrap): (usize, bool),
        colours: Colours,
        (found, picked): (Option<u64>, Option<u64>),
    ) -> Vec<String> {
        let line = self.parse(self.line(number).unwrap_or_default());
        // The level's letter and colour, and the colour of the message.
        let (letter, grave, said) = match line.level {
            Some(Level::Verbose) => ("V", colours.muted, colours.text),
            Some(Level::Debug) => ("D", colours.muted, colours.text),
            Some(Level::Info) => ("I", colours.muted, colours.text),
            // An emulator's own warnings have no tag and are amber whole.
            Some(Level::Warning) if line.tag.is_none() => ("W", colours.warn, colours.warn),
            Some(Level::Warning) => ("W", colours.warn, colours.text),
            Some(Level::Error) => ("E", colours.bad, colours.bad),
            None => (" ", colours.muted, colours.text),
        };

        let ground = if picked == Some(number) {
            colours.chosen
        } else if found == Some(number) {
            mix(colours.ground, colours.accent, 30)
        } else {
            colours.ground
        };

        let indent = Self::indent(&line);
        let mut row = Row::new(ground, columns);

        // The first column stays blank, as the log's left padding.
        row.put(" ", 1);
        row.colour(colours.muted, false);

        if let Some(time) = &line.time {
            row.put(time, 13);
        }

        row.pad(14);
        row.colour(grave, true);
        row.put(letter, 15);
        row.pad(16);

        if let Some(tag) = line.tag {
            row.colour(colours.muted, false);
            row.put(tag, 16 + TAG);
            row.pad(17 + TAG);
        }

        row.colour(said, false);

        let mut rows = Vec::new();

        for (letter, places) in letters(line.message) {
            if row.places + places > columns {
                if !wrap {
                    // The last cell becomes an ellipsis.
                    while row.places + 1 > columns {
                        let Some(last) = row.told.pop() else {
                            break;
                        };

                        row.places -= last.width().unwrap_or(0);
                    }

                    row.pad(columns.saturating_sub(1));
                    row.colour(colours.muted, false);
                    row.put("…", columns);
                    break;
                }

                if rows.len() + 1 == MOST_ROWS {
                    break;
                }

                rows.push(row.finish());
                row = Row::new(ground, columns);
                row.put(" ", 1);
                row.pad(indent.min(columns.saturating_sub(1)));
                row.colour(said, false);
            }

            row.told.push(letter);
            row.places += places;
        }

        rows.push(row.finish());

        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_colours_by_hundredths() {
        assert_eq!(mix(0x000000, 0xffffff, 0), 0x000000);
        assert_eq!(mix(0x000000, 0xffffff, 100), 0xffffff);
        assert_eq!(mix(0x000000, 0x646464, 50), 0x323232);
    }

    #[test]
    fn makes_control_characters_harmless() {
        let cells: Vec<_> = letters("a\tb\u{1b}界").collect();

        assert_eq!(
            cells,
            [
                ('a', 1),
                (' ', 1),
                (' ', 1),
                (' ', 1),
                (' ', 1),
                ('b', 1),
                (' ', 1),
                ('界', 2),
            ]
        );
    }
}
