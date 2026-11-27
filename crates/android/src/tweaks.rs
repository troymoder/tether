//! These settings can also change on the device itself, so they are read
//! back each time and set with Android's own commands.

use adb::Device;
use adb::libadb::Session;
use anyhow::{Result, bail, ensure};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tweaks {
    pub dark: bool,
    /// A percentage of the default.
    pub text_size: u16,
    pub bold: bool,
    pub high_contrast: bool,
    pub inverted: bool,
    pub colors: Colors,
    pub still: bool,
    pub tidy_bar: bool,
    /// A language tag like `en-US`.
    pub locale: Option<String>,
    /// True if the shell can become root, which setting the language needs.
    /// Usually only emulators allow it.
    pub rooted: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Colors {
    #[default]
    Usual,
    Protanomaly,
    Deuteranomaly,
    Tritanomaly,
    Grey,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tweak {
    Dark(bool),
    TextSize(u16),
    Bold(bool),
    HighContrast(bool),
    Inverted(bool),
    Colors(Colors),
    Still(bool),
    TidyBar(bool),
    /// Android's framework restarts to apply it.
    Locale(String),
}

/// Prints every setting as `name=value` on its own line. A setting that was
/// never changed prints `null`.
const READ: &str = "echo night=$(cmd uimode night); \
    for key in 'system font_scale' 'secure font_weight_adjustment' \
        'secure high_text_contrast_enabled' 'secure accessibility_display_inversion_enabled' \
        'secure accessibility_display_daltonizer_enabled' 'secure accessibility_display_daltonizer' \
        'global animator_duration_scale' 'system system_locales'; \
    do echo \"${key#* }=$(settings get $key)\"; done; \
    echo persist=$(getprop persist.sys.locale); \
    echo product=$(getprop ro.product.locale); \
    echo tidy=$(getprop debug.tether.tidy); \
    echo root=$(su 0 id -u 2>/dev/null)";

/// 9:41 is the time in the first iPhone's adverts. Android cannot report
/// demo mode, so a debug property records it, and both are cleared by a
/// reboot.
const TIDY: &str = "settings put global sysui_demo_allowed 1; \
    demo() { am broadcast -a com.android.systemui.demo -e command \"$@\" >/dev/null; }; \
    demo enter; \
    demo clock -e hhmm 0941; \
    demo battery -e level 100 -e plugged false; \
    demo network -e wifi show -e level 4; \
    demo network -e mobile show -e datatype none -e level 4; \
    demo notifications -e visible false; \
    setprop debug.tether.tidy 1";

const UNTIDY: &str = "am broadcast -a com.android.systemui.demo -e command exit >/dev/null; \
    setprop debug.tether.tidy 0";

pub async fn read<S>(device: &Device<S>) -> Result<Tweaks>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    let said = device.script(READ).await?;

    // While Android is booting or restarting, the settings service is not
    // available.
    if said.contains("Failure calling service") || said.contains("Can't find service") {
        bail!("adb failed: android is starting");
    }

    Ok(parse(&said))
}

pub async fn set<S>(device: &Device<S>, tweak: &Tweak) -> Result<String>
where
    S: Session + Clone,
    S::IoError: std::fmt::Debug,
{
    if let Tweak::Locale(locale) = tweak {
        ensure!(
            is_locale(locale),
            "invalid locale: expected a tag such as en-US"
        );

        ensure!(
            read(device).await?.rooted,
            "device locale needs root (emulators only)"
        );
    }

    let (command, said) = command(tweak);

    device.script(&command).await?;

    Ok(said)
}

/// The script that applies a tweak, and the line that describes it.
fn command(tweak: &Tweak) -> (String, String) {
    let put = |place: &str, name: &str, value: &str| format!("settings put {place} {name} {value}");
    let flag = |on: bool| if on { "1" } else { "0" };
    let said = |on: bool, yes: &str, no: &str| if on { yes } else { no }.to_owned();

    match *tweak {
        Tweak::Dark(on) => (
            format!("cmd uimode night {}", if on { "yes" } else { "no" }),
            said(on, "Dark mode is on", "Dark mode is off"),
        ),
        Tweak::TextSize(percent) => {
            let percent = percent.clamp(50, 300);

            (
                put(
                    "system",
                    "font_scale",
                    &format!("{:.2}", f32::from(percent) / 100.0),
                ),
                format!("Text is at {percent}% of its usual size"),
            )
        }
        Tweak::Bold(on) => (
            put(
                "secure",
                "font_weight_adjustment",
                if on { "300" } else { "0" },
            ),
            said(on, "Text is bold", "Text is of its usual weight"),
        ),
        Tweak::HighContrast(on) => (
            put("secure", "high_text_contrast_enabled", flag(on)),
            said(on, "Text has high contrast", "Text has its usual contrast"),
        ),
        Tweak::Inverted(on) => (
            put(
                "secure",
                "accessibility_display_inversion_enabled",
                flag(on),
            ),
            said(on, "Colours are inverted", "Colours are not inverted"),
        ),
        Tweak::Colors(colors) => {
            let mode = match colors {
                Colors::Usual => None,
                Colors::Grey => Some("0"),
                Colors::Protanomaly => Some("11"),
                Colors::Deuteranomaly => Some("12"),
                Colors::Tritanomaly => Some("13"),
            };

            let enabled = put(
                "secure",
                "accessibility_display_daltonizer_enabled",
                flag(mode.is_some()),
            );

            (
                match mode {
                    Some(mode) => format!(
                        "{}; {enabled}",
                        put("secure", "accessibility_display_daltonizer", mode)
                    ),
                    None => enabled,
                },
                "Colours are set".to_owned(),
            )
        }
        Tweak::Still(on) => {
            let scale = flag(!on);
            let all = [
                "window_animation_scale",
                "transition_animation_scale",
                "animator_duration_scale",
            ]
            .map(|name| put("global", name, scale));

            (
                all.join("; "),
                said(on, "Animations are off", "Animations are on"),
            )
        }
        Tweak::TidyBar(on) => (
            if on { TIDY } else { UNTIDY }.to_owned(),
            said(
                on,
                "The status bar is tidy",
                "The status bar shows what is so",
            ),
        ),
        // Android reads the locale only at startup, so the framework is
        // restarted, which takes a few seconds. The sleep gives the settings
        // provider time to persist the new value first. The scrcpy server
        // survives the restart with nothing to show, so it is killed and the
        // page requests the screen again.
        Tweak::Locale(ref locale) => (
            format!(
                "su 0 sh -c 'setprop persist.sys.locale {locale}; \
                 settings put system system_locales {locale}; sleep 3; stop; start; \
                 pkill -f com.genymobile.scrcpy.Server'"
            ),
            format!("Restarting in {locale}"),
        ),
    }
}

fn parse(said: &str) -> Tweaks {
    let get = |name: &str| {
        said.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "null")
    };

    let on = |name: &str| get(name).is_some_and(|value| value != "0");
    let scale = get("font_scale").and_then(|scale| scale.parse::<f32>().ok());

    Tweaks {
        dark: get("night").is_some_and(|night| night.ends_with("yes")),
        text_size: (scale.unwrap_or(1.0) * 100.0).round() as u16,
        bold: on("font_weight_adjustment"),
        high_contrast: on("high_text_contrast_enabled"),
        inverted: on("accessibility_display_inversion_enabled"),
        colors: match get("accessibility_display_daltonizer") {
            _ if !on("accessibility_display_daltonizer_enabled") => Colors::Usual,
            Some("0") => Colors::Grey,
            Some("11") => Colors::Protanomaly,
            Some("13") => Colors::Tritanomaly,
            _ => Colors::Deuteranomaly,
        },
        still: get("animator_duration_scale").is_some_and(|scale| scale.parse() == Ok(0.0)),
        tidy_bar: on("tidy"),
        // The language chosen in settings, then the persisted one, then the
        // factory default.
        locale: [
            get("system_locales").and_then(|locales| locales.split(',').next()),
            get("persist"),
            get("product"),
        ]
        .into_iter()
        .flatten()
        .find(|locale| is_locale(locale))
        .map(str::to_owned),
        rooted: get("root") == Some("0"),
    }
}

/// True if text is a plain language tag like `en-US`, which is checked
/// because it becomes a shell argument.
pub fn is_locale(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 35
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_how_a_device_is_set() {
        let untouched = "night=Night mode: custom_bedtime\nfont_scale=1.0\n\
            font_weight_adjustment=null\nhigh_text_contrast_enabled=null\n\
            accessibility_display_inversion_enabled=null\n\
            accessibility_display_daltonizer_enabled=null\n\
            accessibility_display_daltonizer=null\nanimator_duration_scale=null\n\
            system_locales=null\npersist=\nproduct=en-US\ntidy=\nroot=0\n";

        assert_eq!(
            parse(untouched),
            Tweaks {
                text_size: 100,
                locale: Some("en-US".to_owned()),
                rooted: true,
                ..Tweaks::default()
            }
        );

        let changed = "night=Night mode: yes\nfont_scale=1.3\nfont_weight_adjustment=300\n\
            high_text_contrast_enabled=1\naccessibility_display_inversion_enabled=0\n\
            accessibility_display_daltonizer_enabled=1\naccessibility_display_daltonizer=11\n\
            animator_duration_scale=0.0\nsystem_locales=fr-FR,en-US\npersist=de-DE\n\
            product=en-US\ntidy=1\nroot=\n";

        assert_eq!(
            parse(changed),
            Tweaks {
                dark: true,
                text_size: 130,
                bold: true,
                high_contrast: true,
                inverted: false,
                colors: Colors::Protanomaly,
                still: true,
                tidy_bar: true,
                locale: Some("fr-FR".to_owned()),
                rooted: false,
            }
        );

        let failing = "system_locales=cmd: Failure calling service settings: Broken pipe (32)\n\
            persist=fr-FR\nproduct=en-US\n";

        assert_eq!(parse(failing).locale.as_deref(), Some("fr-FR"));
        assert_eq!(
            parse("system_locales=null\npersist=\nproduct=\n").locale,
            None
        );

        assert!(is_locale("zh-Hans-CN"));
        assert!(!is_locale("en US; reboot"));
        assert!(!is_locale(""));
    }

    #[test]
    fn writes_the_command_for_a_tweak() {
        assert_eq!(
            command(&Tweak::TextSize(1000)),
            (
                "settings put system font_scale 3.00".to_owned(),
                "Text is at 300% of its usual size".to_owned()
            )
        );

        assert_eq!(
            command(&Tweak::Colors(Colors::Grey)).0,
            "settings put secure accessibility_display_daltonizer 0; \
             settings put secure accessibility_display_daltonizer_enabled 1"
        );

        assert_eq!(
            command(&Tweak::Colors(Colors::Usual)).0,
            "settings put secure accessibility_display_daltonizer_enabled 0"
        );
    }
}
