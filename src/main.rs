use std::fmt;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ddc_hi::{Backend, Ddc, Display, DisplayInfo, VcpValue};

const RETRY_ATTEMPTS: usize = 20;
const RETRY_DELAY: Duration = Duration::from_millis(100);

#[derive(Parser, Debug)]
#[command(author, version, about = "Minimalist DDC/CI monitor control")]
struct Cli {
    /// DDC backend to use. Auto selects WinAPI on Windows.
    #[arg(short, long, value_enum, default_value_t = BackendChoice::Auto, global = true)]
    backend: BackendChoice,

    /// Only act on displays whose ID, model, manufacturer, or serial contains this text.
    #[arg(short, long, global = true)]
    display: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List detected DDC/CI displays.
    Displays,

    /// List every VCP feature advertised by the display.
    Controls,

    /// Print the raw MCCS capabilities string advertised by the display.
    Capabilities,

    /// Read or set luminance/brightness (VCP 0x10).
    Brightness {
        /// Absolute value, or a signed relative change such as +10 or -10.
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set contrast (VCP 0x12).
    Contrast {
        /// Absolute value, or a signed relative change such as +10 or -10.
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set speaker volume (VCP 0x62).
    Volume {
        /// Absolute value, or a signed relative change such as +5 or -5.
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set audio mute state (VCP 0x8D).
    Mute {
        /// Monitor-specific absolute value; inspect `controls` for supported values.
        value: Option<ValueChange>,
    },

    /// Read or select the video input (VCP 0x60).
    Input {
        /// Monitor-specific absolute value, decimal or hexadecimal (for example 0x0F).
        value: Option<ValueChange>,
    },

    /// Read or set the display power mode (VCP 0xD6).
    Power {
        /// Monitor-specific absolute value. Warning: some values power the display off.
        value: Option<ValueChange>,
    },

    /// Read or select a color-temperature preset (VCP 0x14).
    ColorPreset {
        /// Monitor-specific absolute value; inspect `controls` for supported values.
        value: Option<ValueChange>,
    },

    /// Read or set the requested color temperature (VCP 0x0C).
    ColorTemperature {
        /// Absolute value, or a signed relative change such as +1 or -1.
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set red video gain (VCP 0x16).
    RedGain {
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set green video gain (VCP 0x18).
    GreenGain {
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set blue video gain (VCP 0x1A).
    BlueGain {
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or set speaker treble (VCP 0x87).
    Treble {
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Read or select the monitor OSD language (VCP 0xCC).
    OsdLanguage {
        /// Monitor-specific absolute value; inspect `controls` for supported values.
        value: Option<ValueChange>,
    },

    /// Read or set any raw VCP feature code.
    Vcp {
        /// VCP feature code in hexadecimal, with or without a 0x prefix.
        code: VcpCode,

        /// Absolute value, or a signed relative change such as +10 or -10.
        #[arg(allow_hyphen_values = true)]
        value: Option<ValueChange>,
    },

    /// Ask the monitor to save its current settings.
    Save,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BackendChoice {
    Auto,
    Winapi,
    Nvapi,
    I2c,
    Macos,
}

impl BackendChoice {
    fn resolve(self) -> Backend {
        match self {
            Self::Winapi => Backend::WinApi,
            Self::Nvapi => Backend::Nvapi,
            Self::I2c => Backend::I2cDevice,
            Self::Macos => Backend::MacOS,
            Self::Auto => default_backend(),
        }
    }
}

#[cfg(target_os = "windows")]
fn default_backend() -> Backend {
    Backend::WinApi
}

#[cfg(target_os = "linux")]
fn default_backend() -> Backend {
    Backend::I2cDevice
}

#[cfg(target_os = "macos")]
fn default_backend() -> Backend {
    Backend::MacOS
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
compile_error!("mmc does not have a default DDC backend for this operating system");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueChange {
    Absolute(u16),
    Relative(i32),
}

impl FromStr for ValueChange {
    type Err = String;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(rest) = input.strip_prefix('+') {
            let value = parse_unsigned(rest)?;
            return Ok(Self::Relative(i32::from(value)));
        }

        if let Some(rest) = input.strip_prefix('-') {
            let value = parse_unsigned(rest)?;
            return Ok(Self::Relative(-i32::from(value)));
        }

        parse_unsigned(input).map(Self::Absolute)
    }
}

fn parse_unsigned(input: &str) -> std::result::Result<u16, String> {
    if input.is_empty() {
        return Err("expected a number after the sign".to_owned());
    }

    let parsed = if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16)
    } else {
        input.parse::<u16>()
    };

    parsed.map_err(|_| {
        format!("invalid value `{input}`; use decimal or a 0x-prefixed hexadecimal value")
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VcpCode(u8);

impl FromStr for VcpCode {
    type Err = String;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let digits = input
            .strip_prefix("0x")
            .or_else(|| input.strip_prefix("0X"))
            .unwrap_or(input);

        u8::from_str_radix(digits, 16)
            .map(Self)
            .map_err(|_| format!("invalid VCP code `{input}`; expected a hexadecimal byte"))
    }
}

#[derive(Clone, Copy)]
struct Control {
    name: &'static str,
    code: u8,
    relative: bool,
}

impl Control {
    const fn continuous(name: &'static str, code: u8) -> Self {
        Self {
            name,
            code,
            relative: true,
        }
    }

    const fn discrete(name: &'static str, code: u8) -> Self {
        Self {
            name,
            code,
            relative: false,
        }
    }

    const fn raw(code: u8) -> Self {
        Self {
            name: "raw VCP feature",
            code,
            relative: true,
        }
    }
}

const BRIGHTNESS: Control = Control::continuous("brightness", 0x10);
const CONTRAST: Control = Control::continuous("contrast", 0x12);
const COLOR_TEMPERATURE: Control = Control::continuous("color temperature", 0x0C);
const COLOR_PRESET: Control = Control::discrete("color preset", 0x14);
const RED_GAIN: Control = Control::continuous("red gain", 0x16);
const GREEN_GAIN: Control = Control::continuous("green gain", 0x18);
const BLUE_GAIN: Control = Control::continuous("blue gain", 0x1A);
const INPUT: Control = Control::discrete("input source", 0x60);
const VOLUME: Control = Control::continuous("volume", 0x62);
const TREBLE: Control = Control::continuous("treble", 0x87);
const MUTE: Control = Control::discrete("audio mute", 0x8D);
const OSD_LANGUAGE: Control = Control::discrete("OSD language", 0xCC);
const POWER: Control = Control::discrete("power mode", 0xD6);

const KNOWN_CONTROLS: &[Control] = &[
    BRIGHTNESS,
    CONTRAST,
    COLOR_TEMPERATURE,
    COLOR_PRESET,
    RED_GAIN,
    GREEN_GAIN,
    BLUE_GAIN,
    INPUT,
    VOLUME,
    TREBLE,
    MUTE,
    OSD_LANGUAGE,
    POWER,
];

fn main() -> Result<()> {
    let cli = Cli::parse();
    let displays = select_displays(&cli)?;

    match cli.command {
        Command::Displays => list_displays(displays),
        Command::Controls => list_controls(displays),
        Command::Capabilities => print_capabilities(displays),
        Command::Brightness { value } => control(displays, BRIGHTNESS, value),
        Command::Contrast { value } => control(displays, CONTRAST, value),
        Command::Volume { value } => control(displays, VOLUME, value),
        Command::Mute { value } => control(displays, MUTE, value),
        Command::Input { value } => control(displays, INPUT, value),
        Command::Power { value } => control(displays, POWER, value),
        Command::ColorPreset { value } => control(displays, COLOR_PRESET, value),
        Command::ColorTemperature { value } => control(displays, COLOR_TEMPERATURE, value),
        Command::RedGain { value } => control(displays, RED_GAIN, value),
        Command::GreenGain { value } => control(displays, GREEN_GAIN, value),
        Command::BlueGain { value } => control(displays, BLUE_GAIN, value),
        Command::Treble { value } => control(displays, TREBLE, value),
        Command::OsdLanguage { value } => control(displays, OSD_LANGUAGE, value),
        Command::Vcp { code, value } => control(displays, Control::raw(code.0), value),
        Command::Save => save_settings(displays),
    }
}

fn select_displays(cli: &Cli) -> Result<Vec<Display>> {
    let backend = cli.backend.resolve();
    let filter = cli.display.as_deref().map(str::to_lowercase);

    let displays: Vec<_> = Display::enumerate()
        .into_iter()
        .filter(|display| display.info.backend == backend)
        .filter(|display| {
            filter
                .as_deref()
                .is_none_or(|needle| display_matches(display, needle))
        })
        .collect();

    if displays.is_empty() {
        let filter_message = cli
            .display
            .as_deref()
            .map(|value| format!(" matching `{value}`"))
            .unwrap_or_default();
        bail!("no DDC/CI displays found using the {backend} backend{filter_message}");
    }

    Ok(displays)
}

fn display_matches(display: &Display, needle: &str) -> bool {
    let fields = [
        Some(display.info.id.as_str()),
        display.info.manufacturer_id.as_deref(),
        display.info.model_name.as_deref(),
        display.info.serial_number.as_deref(),
    ];

    fields
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(needle))
}

fn display_label(display: &Display) -> String {
    let identity = display
        .info
        .model_name
        .as_deref()
        .or(display.info.manufacturer_id.as_deref())
        .unwrap_or(display.info.id.as_str());
    format!("[{}] {identity}", display.info.backend)
}

fn list_displays(displays: Vec<Display>) -> Result<()> {
    for display in displays {
        println!("{}", display_label(&display));
        println!("  id: {}", display.info.id);
        if let Some(serial) = display.info.serial_number.as_deref() {
            println!("  serial: {serial}");
        }
    }
    Ok(())
}

fn list_controls(displays: Vec<Display>) -> Result<()> {
    for_each_display(displays, |display| {
        let raw = retry_ddc(|| display.handle.capabilities_string())
            .context("failed to retrieve the monitor capabilities string")?;
        let capabilities = mccs_caps::parse_capabilities(&raw)
            .context("the monitor returned an invalid capabilities string")?;
        let info = DisplayInfo::from_capabilities(
            display.info.backend,
            display.info.id.clone(),
            &capabilities,
        );

        println!("{}", display_label(display));
        if let Some(version) = capabilities.mccs_version.as_ref() {
            println!("  MCCS {version}");
        }

        for (&code, advertised) in &capabilities.vcp_features {
            let known_name = known_control_name(code);
            let descriptor = info.mccs_database.get(code);
            let name = known_name
                .or_else(|| descriptor.and_then(|item| item.name.as_deref()))
                .unwrap_or("unknown/manufacturer-specific");
            let values = advertised
                .values
                .keys()
                .map(|value| format!("0x{value:02X}"))
                .collect::<Vec<_>>()
                .join(", ");

            if values.is_empty() {
                println!("  0x{code:02X}  {name}");
            } else {
                println!("  0x{code:02X}  {name}  values: {values}");
            }
        }

        Ok(())
    })
}

fn print_capabilities(displays: Vec<Display>) -> Result<()> {
    for_each_display(displays, |display| {
        let raw = retry_ddc(|| display.handle.capabilities_string())
            .context("failed to retrieve the monitor capabilities string")?;
        println!("{}", display_label(display));
        println!("{}", String::from_utf8_lossy(&raw));
        Ok(())
    })
}

fn control(displays: Vec<Display>, feature: Control, change: Option<ValueChange>) -> Result<()> {
    for_each_display(displays, |display| match change {
        None => {
            let value = get_vcp(display, feature.code).with_context(|| {
                format!(
                    "failed to read {} (VCP 0x{:02X})",
                    feature.name, feature.code
                )
            })?;
            if feature.relative {
                println!(
                    "{} {}: {} / {} (VCP 0x{:02X})",
                    display_label(display),
                    feature.name,
                    value.value(),
                    value.maximum(),
                    feature.code
                );
            } else {
                println!(
                    "{} {}: {} (VCP 0x{:02X})",
                    display_label(display),
                    feature.name,
                    value.value(),
                    feature.code
                );
            }
            Ok(())
        }
        Some(ValueChange::Absolute(target)) => {
            set_vcp(display, feature.code, target).with_context(|| {
                format!(
                    "failed to set {} (VCP 0x{:02X})",
                    feature.name, feature.code
                )
            })?;
            println!(
                "{} {}: set to {} (VCP 0x{:02X})",
                display_label(display),
                feature.name,
                target,
                feature.code
            );
            Ok(())
        }
        Some(ValueChange::Relative(delta)) => {
            if !feature.relative {
                bail!(
                    "{} (VCP 0x{:02X}) is a discrete control and does not accept relative values",
                    feature.name,
                    feature.code
                );
            }

            let current = get_vcp(display, feature.code).with_context(|| {
                format!(
                    "failed to read {} before applying a relative change",
                    feature.name
                )
            })?;
            let target = apply_relative(current.value(), current.maximum(), delta);
            set_vcp(display, feature.code, target).with_context(|| {
                format!(
                    "failed to set {} (VCP 0x{:02X})",
                    feature.name, feature.code
                )
            })?;
            println!(
                "{} {}: {} -> {} ({delta:+}, max {}; VCP 0x{:02X})",
                display_label(display),
                feature.name,
                current.value(),
                target,
                current.maximum(),
                feature.code
            );
            Ok(())
        }
    })
}

fn save_settings(displays: Vec<Display>) -> Result<()> {
    for_each_display(displays, |display| {
        retry_ddc(|| display.handle.save_current_settings())
            .context("the monitor did not save its current settings")?;
        println!("{} settings saved", display_label(display));
        Ok(())
    })
}

fn for_each_display<F>(mut displays: Vec<Display>, mut operation: F) -> Result<()>
where
    F: FnMut(&mut Display) -> Result<()>,
{
    let mut errors = Vec::new();

    for display in &mut displays {
        if let Err(error) = operation(display) {
            errors.push(format!("{}: {error:#}", display_label(display)));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(errors.join("\n")))
    }
}

fn get_vcp(display: &mut Display, code: u8) -> Result<VcpValue> {
    retry_ddc(|| display.handle.get_vcp_feature(code))
}

fn set_vcp(display: &mut Display, code: u8, value: u16) -> Result<()> {
    retry_ddc(|| display.handle.set_vcp_feature(code, value))
}

fn retry_ddc<T, F>(mut operation: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut last_error = None;

    for attempt in 0..RETRY_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }

        if attempt + 1 < RETRY_ATTEMPTS {
            thread::sleep(RETRY_DELAY);
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("DDC operation failed without an error")))
}

fn apply_relative(current: u16, maximum: u16, delta: i32) -> u16 {
    let upper_bound = if maximum == 0 {
        i32::from(u16::MAX)
    } else {
        i32::from(maximum)
    };
    (i32::from(current) + delta).clamp(0, upper_bound) as u16
}

fn known_control_name(code: u8) -> Option<&'static str> {
    KNOWN_CONTROLS
        .iter()
        .find(|control| control.code == code)
        .map(|control| control.name)
        .or(match code {
            0x02 => Some("new control value"),
            0x0B => Some("color-temperature increment"),
            0xAC => Some("horizontal frequency"),
            0xAE => Some("vertical frequency"),
            0xB2 => Some("flat-panel subpixel layout"),
            0xB6 => Some("display technology type"),
            0xC6 => Some("application enable key"),
            0xC8 => Some("display controller ID"),
            0xCA => Some("OSD/button control"),
            0xDF => Some("MCCS version"),
            0xFD | 0xFF => Some("manufacturer-specific"),
            _ => None,
        })
}

impl fmt::Display for ValueChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute(value) => write!(formatter, "{value}"),
            Self::Relative(value) => write!(formatter, "{value:+}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_absolute_decimal_value() {
        assert_eq!("50".parse(), Ok(ValueChange::Absolute(50)));
    }

    #[test]
    fn parses_absolute_hex_value() {
        assert_eq!("0x0F".parse(), Ok(ValueChange::Absolute(15)));
    }

    #[test]
    fn parses_positive_relative_value() {
        assert_eq!("+10".parse(), Ok(ValueChange::Relative(10)));
    }

    #[test]
    fn parses_negative_relative_value() {
        assert_eq!("-10".parse(), Ok(ValueChange::Relative(-10)));
    }

    #[test]
    fn relative_values_are_clamped_to_reported_range() {
        assert_eq!(apply_relative(95, 100, 10), 100);
        assert_eq!(apply_relative(5, 100, -10), 0);
        assert_eq!(apply_relative(50, 100, 10), 60);
    }

    #[test]
    fn parses_vcp_codes_as_hexadecimal() {
        assert_eq!("10".parse(), Ok(VcpCode(0x10)));
        assert_eq!("0xD6".parse(), Ok(VcpCode(0xD6)));
    }
}
