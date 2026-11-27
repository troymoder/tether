//! A log line names its process only by pid. The device's process names and
//! the app in front are listed here too, so an app's lines can be picked out.

use std::collections::VecDeque;

use adb::libadb::Session;
use adb::{Device, Input, Output, Said};
use anyhow::Result;

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub name: String,
}

pub struct Following<S> {
    pub kept: Vec<String>,
    /// Shut it to stop following, which also ends a pending read of `lines`.
    pub stop: Input<S>,
    pub lines: Lines<S>,
}

pub struct Lines<S> {
    output: Output<S>,
    pending: Vec<u8>,
    /// The kept lines that share the last kept timestamp, which logcat
    /// repeats when following from that time.
    again: VecDeque<String>,
}

pub async fn follow<S>(device: &Device<S>, most: usize) -> Result<Following<S>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let most = most.to_string();
    let dumped = device
        .shell(&adb::command(["logcat", "-v", "time", "-t", &most])?, None)
        .await?;
    let kept: Vec<String> = String::from_utf8_lossy(&dumped)
        .lines()
        .map(str::to_owned)
        .collect();
    let (from, again) = since(&kept);

    // Following from the last kept line's time loses no line logged in
    // between. With no kept lines, only new lines are followed.
    let command = adb::command(["logcat", "-v", "time", "-T", from.unwrap_or("1")])?;
    let process = device.spawn(&command, None).await?;

    Ok(Following {
        kept,
        stop: process.input,
        lines: Lines {
            output: process.output,
            pending: Vec::new(),
            again,
        },
    })
}

impl<S> Lines<S>
where
    S: Session,
    S::IoError: std::fmt::Debug,
{
    pub async fn next(&mut self) -> Result<Option<Vec<String>>> {
        loop {
            let Some(said) = self.output.next().await? else {
                return Ok(None);
            };

            let Said::Stdout(bytes) = said else {
                continue;
            };

            self.pending.extend_from_slice(&bytes);

            let lines: Vec<String> = whole_lines(&mut self.pending)
                .into_iter()
                .filter(|line| new(&mut self.again, line))
                .collect();

            if !lines.is_empty() {
                return Ok(Some(lines));
            }
        }
    }
}

/// Lines are split on bytes, because a log is not always valid UTF-8.
fn whole_lines(pending: &mut Vec<u8>) -> Vec<String> {
    let Some(end) = pending.iter().rposition(|byte| *byte == b'\n') else {
        return Vec::new();
    };

    let whole: Vec<u8> = pending.drain(..=end).collect();

    String::from_utf8_lossy(&whole)
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// A line of `logcat -v time` starts with a time like `11-20 01:47:12.345`.
fn time(line: &str) -> Option<&str> {
    let time = line.get(..18)?;
    let shaped = time.bytes().enumerate().all(|(at, byte)| match at {
        2 => byte == b'-',
        5 => byte == b' ',
        8 | 11 => byte == b':',
        14 => byte == b'.',
        _ => byte.is_ascii_digit(),
    });

    shaped.then_some(time)
}

/// The time of the last kept line, to follow from, and the kept lines from
/// the first one with that time, which logcat will repeat.
fn since(kept: &[String]) -> (Option<&str>, VecDeque<String>) {
    let Some(last) = kept.iter().rev().find_map(|line| time(line)) else {
        return (None, VecDeque::new());
    };

    let first = kept
        .iter()
        .position(|line| time(line) == Some(last))
        .unwrap_or(kept.len());

    (Some(last), kept[first..].iter().cloned().collect())
}

/// logcat may print a buffer heading among the repeats.
fn new(again: &mut VecDeque<String>, line: &str) -> bool {
    if again.is_empty() {
        true
    } else if line.starts_with("--------- ") {
        false
    } else if again.front().is_some_and(|kept| kept == line) {
        again.pop_front();
        false
    } else {
        again.clear();
        true
    }
}

pub async fn processes<S>(device: &Device<S>) -> Result<Vec<Process>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let list = device.sh(["ps", "-A", "-o", "PID,NAME"]).await?;

    Ok(parse_processes(&list))
}

fn parse_processes(list: &str) -> Vec<Process> {
    list.lines()
        .filter_map(|line| {
            let (pid, name) = line.trim().split_once(char::is_whitespace)?;

            Some(Process {
                pid: pid.parse().ok()?,
                name: name.trim().to_owned(),
            })
        })
        .collect()
}

const FRONT: &str = "dumpsys activity activities | grep -m1 topResumedActivity; \
    cmd package resolve-activity --brief -c android.intent.category.HOME -a android.intent.action.MAIN \
    | tail -n 1";

pub async fn front<S>(device: &Device<S>) -> Result<String>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    Ok(parse_front(&device.script(FRONT).await?))
}

/// [`FRONT`] prints
/// `topResumedActivity=ActivityRecord{2356 u0 com.example.app/.Main t8}`,
/// then `com.example.launcher/.Home`. The result is empty when the home
/// screen is in front.
fn parse_front(answer: &str) -> String {
    let package = |line: &str| {
        let (package, _) = line
            .split_whitespace()
            .find_map(|word| word.split_once('/'))?;

        Some(package.to_owned())
    };

    let (top, others): (Vec<&str>, Vec<&str>) = answer
        .lines()
        .partition(|line| line.contains("topResumedActivity"));
    let top = top.into_iter().find_map(package);
    let home = others.into_iter().filter_map(package).next_back();

    top.filter(|top| Some(top) != home.as_ref())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_process_list() {
        let list = "  PID NAME\n    1 init\n  668 system_server\n 4301 com.example.app:sync\n";
        let process = |pid, name: &str| Process {
            pid,
            name: name.to_owned(),
        };

        assert_eq!(
            parse_processes(list),
            [
                process(1, "init"),
                process(668, "system_server"),
                process(4301, "com.example.app:sync"),
            ]
        );
    }

    #[test]
    fn tells_the_app_in_front_from_the_home_screen() {
        let home = "com.example.launcher/.Home\n";
        let in_app = "    topResumedActivity=ActivityRecord{2356 u0 com.example.app/.Main t8}\n\
            Failed to write while dumping service activity: Broken pipe\n";
        let at_home = "  topResumedActivity=ActivityRecord{24 u0 com.example.launcher/.Home t6}\n";

        assert_eq!(parse_front(&format!("{in_app}{home}")), "com.example.app");
        assert_eq!(parse_front(&format!("{at_home}{home}")), "");
        assert_eq!(parse_front(home), "");
    }

    #[test]
    fn takes_whole_lines_as_they_come() {
        let mut pending = b"one\r\ntw".to_vec();

        assert_eq!(whole_lines(&mut pending), ["one"]);
        assert!(whole_lines(&mut pending).is_empty());

        pending.extend_from_slice(b"o \xff\nthree\n");

        assert_eq!(whole_lines(&mut pending), ["two \u{fffd}", "three"]);
        assert!(pending.is_empty());
    }

    #[test]
    fn follows_on_from_what_is_kept() {
        let kept: Vec<String> = [
            "--------- beginning of main",
            "11-20 01:47:12.345 I/a(  1): one",
            "11-20 01:47:13.000 I/a(  1): two",
            "11-20 01:47:13.000 I/a(  1): three",
            "\tat a stack trace",
        ]
        .map(str::to_owned)
        .into();

        let (from, mut again) = since(&kept);

        assert_eq!(from, Some("11-20 01:47:13.000"));
        assert_eq!(again.len(), 3);

        let followed = [
            "--------- beginning of main",
            "11-20 01:47:13.000 I/a(  1): two",
            "11-20 01:47:13.000 I/a(  1): three",
            "\tat a stack trace",
            "11-20 01:47:13.000 I/a(  1): four",
            "11-20 01:47:13.000 I/a(  1): two",
        ];

        let new: Vec<&str> = followed
            .into_iter()
            .filter(|line| new(&mut again, line))
            .collect();

        assert_eq!(
            new,
            [
                "11-20 01:47:13.000 I/a(  1): four",
                "11-20 01:47:13.000 I/a(  1): two"
            ]
        );

        assert_eq!(since(&["--------- beginning of main".to_owned()]).0, None);
        assert_eq!(time("11-20 01:47:12.345 I/a"), Some("11-20 01:47:12.345"));
        assert_eq!(time("Fatal signal"), None);
    }
}
