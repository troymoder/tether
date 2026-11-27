use adb::libadb::Session;
use adb::{Device, Process, Terminal};
use anyhow::Result;

const STARTUP: &str = "/data/local/tmp/.tether_mkshrc";
/// `ls` and `grep` print no colour unless asked.
const STARTUP_SAYS: &str = "\
[ -r /system/etc/mkshrc ] && . /system/etc/mkshrc
alias ls='ls --color=auto'
alias grep='grep --color=auto'
";

pub async fn open<S>(device: &Device<S>, (columns, rows): (u16, u16)) -> Result<Process<S>>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    // If the startup file cannot be written, the device's default shell is used.
    let write = format!("cat > {STARTUP}");
    let coloured = device
        .shell(&write, Some(STARTUP_SAYS.as_bytes()))
        .await
        .is_ok();

    // The page's terminal is xterm.js, which understands xterm-256color.
    let terminal = Terminal {
        term: "xterm-256color",
        rows,
        cols: columns,
    };

    let command = if coloured {
        format!("ENV={STARTUP} exec sh")
    } else {
        String::new()
    };

    device.spawn(&command, Some(terminal)).await
}
