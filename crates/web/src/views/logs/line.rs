use crate::util::clock;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(super) enum Level {
    Verbose,
    Debug,
    Info,
    Warning,
    Error,
}

impl Level {
    pub(super) const ALL: [(Level, &str); 5] = [
        (Level::Verbose, "verbose"),
        (Level::Debug, "debug"),
        (Level::Info, "info"),
        (Level::Warning, "warning"),
        (Level::Error, "error"),
    ];

    pub(super) fn name(self) -> &'static str {
        Self::ALL[self as usize].1
    }
}

/// A line in an unknown form is all message.
#[derive(Debug, PartialEq)]
pub(super) struct Line<'a> {
    /// The time of day as `HH:MM:SS.mmm`.
    pub(super) time: Option<String>,
    pub(super) level: Option<Level>,
    /// None for an emulator's own output, and for a note from logcat.
    pub(super) tag: Option<&'a str>,
    pub(super) pid: Option<u32>,
    pub(super) message: &'a str,
}

/// Parses a line as `adb logcat -v time` prints it:
/// `11-20 01:47:12.345 W/ActivityManager(  668): Slow operation`.
pub(super) fn device_line(line: &str) -> Line<'_> {
    let parsed = (|| {
        let (date, rest) = line.split_once(' ')?;
        let (time, rest) = rest.split_once(' ')?;
        let (level, rest) = rest.split_once('/')?;
        let (tag, message) = rest.split_once("):")?;

        let level = match level {
            "V" => Level::Verbose,
            "D" => Level::Debug,
            "I" => Level::Info,
            "W" => Level::Warning,
            "E" | "F" => Level::Error,
            _ => return None,
        };

        (date.len() == 5 && time.len() == 12).then(|| Line {
            time: Some(time.to_owned()),
            level: Some(level),
            // The tag is followed by the process id in brackets. A line
            // whose tag is blank is drawn as one without a tag.
            tag: Some(tag.rsplit_once('(').map_or(tag, |(tag, _)| tag).trim())
                .filter(|tag| !tag.is_empty()),
            pid: tag
                .rsplit_once('(')
                .and_then(|(_, pid)| pid.trim().parse().ok()),
            message: message.strip_prefix(' ').unwrap_or(message),
        })
    })();

    parsed.unwrap_or(Line {
        time: None,
        level: None,
        tag: None,
        pid: None,
        message: line,
    })
}

/// Parses a line of an emulator's output as tetherd keeps it: milliseconds
/// since the epoch, a tab, and what the emulator printed, mostly in the form
/// `WARNING      | Something happened`.
pub(super) fn emulator_line(line: &str) -> Line<'_> {
    let (time, text) = match line.split_once('\t') {
        Some((milliseconds, text)) => (milliseconds.parse().ok().map(clock), text),
        None => (None, line),
    };

    let leveled = text.split_once('|').and_then(|(label, message)| {
        let level = match label.trim() {
            "VERBOSE" => Level::Verbose,
            "DEBUG" => Level::Debug,
            "INFO" | "USER_INFO" => Level::Info,
            "WARNING" | "USER_WARNING" => Level::Warning,
            "ERROR" | "USER_ERROR" | "FATAL" => Level::Error,
            _ => return None,
        };

        Some((level, message.strip_prefix(' ').unwrap_or(message)))
    });

    Line {
        time,
        level: leveled.map(|(level, _)| level),
        tag: None,
        pid: None,
        message: leveled.map_or(text, |(_, message)| message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_device_line() {
        let line = device_line("11-20 01:47:12.345 W/ActivityManager(  668): Slow operation");

        assert_eq!(
            line,
            Line {
                time: Some("01:47:12.345".to_owned()),
                level: Some(Level::Warning),
                tag: Some("ActivityManager"),
                pid: Some(668),
                message: "Slow operation",
            }
        );
    }

    #[test]
    fn keeps_an_unknown_device_line_as_message() {
        let line = device_line("--------- beginning of main");

        assert_eq!(line.level, None);
        assert_eq!(line.message, "--------- beginning of main");
    }

    #[test]
    fn parses_an_emulator_line_without_a_time() {
        let line = emulator_line("WARNING      | Something happened");

        assert_eq!(line.level, Some(Level::Warning));
        assert_eq!(line.message, "Something happened");
        assert_eq!(line.time, None);
    }
}
