use std::fmt;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use ddc_hi::Display;
use hidapi::{DeviceInfo, HidApi, HidDevice};

const GIGABYTE_HID_VENDOR_ID: u16 = 0x0BDA;
const GIGABYTE_HID_PRODUCT_ID: u16 = 0x1100;
const MO27Q28G_EDID_ID: &str = "GBT273C";
const MO27Q28G_MODEL: &str = "MO27Q28G";
const REPORT_DATA_LEN: usize = 192;
const REPORT_LEN: usize = REPORT_DATA_LEN + 1;
const DDC_OFFSET: usize = 1 + 0x40;
const IO_DELAY: Duration = Duration::from_millis(60);
const QUERY_ATTEMPTS: usize = 3;

#[derive(Args, Debug)]
pub struct GigabyteArgs {
    /// HID device index shown by `gigabyte devices`.
    #[arg(long, default_value_t = 0, global = true)]
    device: usize,

    /// Allow commands when an attached MO27Q28G cannot be confirmed through EDID.
    #[arg(long, global = true)]
    force: bool,

    #[command(subcommand)]
    command: GigabyteCommand,
}

#[derive(Subcommand, Debug)]
enum GigabyteCommand {
    /// List compatible Realtek/Gigabyte USB HID endpoints.
    Devices,

    /// List the implemented MO27Q28G controls and accepted values.
    Controls,

    /// Read a useful summary of the monitor's current configuration.
    Status,

    /// Read one Gigabyte or standard HID control.
    Get {
        #[arg(value_enum)]
        control: ControlName,
    },

    /// Change one Gigabyte or standard HID control, then verify it when possible.
    Set {
        #[arg(value_enum)]
        control: ControlName,

        /// Symbolic, decimal, hexadecimal, or signed relative value.
        #[arg(allow_hyphen_values = true)]
        value: String,
    },

    /// Read OLED maintenance information or explicitly start/stop Pixel Clean.
    Oled {
        #[command(subcommand)]
        command: OledCommand,
    },
}

#[derive(Subcommand, Debug)]
enum OledCommand {
    /// Show panel usage, Pixel Clean state, and completed-clean count.
    Status,

    /// Control the monitor's manual Pixel Clean operation.
    PixelClean {
        #[command(subcommand)]
        command: PixelCleanCommand,
    },
}

#[derive(Subcommand, Debug)]
enum PixelCleanCommand {
    /// Start Pixel Clean. The monitor may blank or become temporarily unavailable.
    Start {
        /// Confirm the disruptive maintenance operation.
        #[arg(long)]
        yes: bool,
    },

    /// Ask the monitor to stop an active manual Pixel Clean operation.
    Stop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ControlName {
    Brightness,
    Contrast,
    Volume,
    Sharpness,
    BlackEqualizer,
    ColorTemperature,
    RedGain,
    GreenGain,
    BlueGain,
    Gamma,
    ColorVibrance,
    UltraClear,
    LowBlueLight,
    Vrr,
    SuperResolution,
    PipMode,
    PipSource,
    PipSwitch,
    PipAudioSwitch,
    PipSize,
    PipLocation,
    Dashboard,
    DashboardLocation,
    RefreshRateOverlay,
    TimerMode,
    TimerCurrent,
    TimerValue,
    TimerState,
    Counter,
    CounterCurrent,
    CounterValue,
    OverlayLocation,
    PictureMode,
    Input,
    AudioInput,
    OsdTransparency,
    OsdTime,
    LedIndicator,
    QuickUp,
    QuickDown,
    QuickRight,
    QuickLeft,
    Crosshair,
    FirmwareVersion,
    Hdr,
    KvmDevice,
    KvmButton,
    KvmUsbBInput,
    KvmUsbCInput,
    PanelHours,
    PixelCleanState,
    PixelCleanCount,
}

const ALL_CONTROLS: &[ControlName] = &[
    ControlName::Brightness,
    ControlName::Contrast,
    ControlName::Volume,
    ControlName::Sharpness,
    ControlName::BlackEqualizer,
    ControlName::ColorTemperature,
    ControlName::RedGain,
    ControlName::GreenGain,
    ControlName::BlueGain,
    ControlName::Gamma,
    ControlName::ColorVibrance,
    ControlName::UltraClear,
    ControlName::LowBlueLight,
    ControlName::Vrr,
    ControlName::SuperResolution,
    ControlName::PipMode,
    ControlName::PipSource,
    ControlName::PipSwitch,
    ControlName::PipAudioSwitch,
    ControlName::PipSize,
    ControlName::PipLocation,
    ControlName::Dashboard,
    ControlName::DashboardLocation,
    ControlName::RefreshRateOverlay,
    ControlName::TimerMode,
    ControlName::TimerCurrent,
    ControlName::TimerValue,
    ControlName::TimerState,
    ControlName::Counter,
    ControlName::CounterCurrent,
    ControlName::CounterValue,
    ControlName::OverlayLocation,
    ControlName::PictureMode,
    ControlName::Input,
    ControlName::AudioInput,
    ControlName::OsdTransparency,
    ControlName::OsdTime,
    ControlName::LedIndicator,
    ControlName::QuickUp,
    ControlName::QuickDown,
    ControlName::QuickRight,
    ControlName::QuickLeft,
    ControlName::Crosshair,
    ControlName::FirmwareVersion,
    ControlName::Hdr,
    ControlName::KvmDevice,
    ControlName::KvmButton,
    ControlName::KvmUsbBInput,
    ControlName::KvmUsbCInput,
    ControlName::PanelHours,
    ControlName::PixelCleanState,
    ControlName::PixelCleanCount,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Address {
    Standard(u8),
    Vendor(u8),
}

impl fmt::Display for Address {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard(code) => write!(formatter, "VCP 0x{code:02X}"),
            Self::Vendor(selector) => write!(formatter, "VCP 0xE0 / selector 0x{selector:02X}"),
        }
    }
}

#[derive(Clone, Copy)]
struct Choice {
    value: u16,
    name: &'static str,
}

#[derive(Clone, Copy)]
struct ControlSpec {
    name: &'static str,
    address: Address,
    min: u16,
    max: u16,
    readable: bool,
    writable: bool,
    relative: bool,
    verify: bool,
    choices: &'static [Choice],
    description: &'static str,
}

const fn choice(value: u16, name: &'static str) -> Choice {
    Choice { value, name }
}

const ON_OFF: &[Choice] = &[choice(0, "off"), choice(1, "on")];
const COLOR_TEMPERATURES: &[Choice] = &[
    choice(0, "cool"),
    choice(1, "normal"),
    choice(2, "warm"),
    choice(3, "user"),
];
const GAMMA: &[Choice] = &[
    choice(0, "off"),
    choice(1, "1.8"),
    choice(2, "2.0"),
    choice(3, "2.2"),
    choice(4, "2.4"),
    choice(5, "2.6"),
];
const PIP_MODES: &[Choice] = &[choice(0, "off"), choice(1, "pip"), choice(2, "pbp")];
const INPUTS: &[Choice] = &[
    choice(0, "hdmi1"),
    choice(1, "hdmi2"),
    choice(2, "displayport"),
    choice(3, "usb-c"),
];
const PIP_SIZES: &[Choice] = &[choice(0, "large"), choice(1, "medium"), choice(2, "small")];
const PIP_LOCATIONS: &[Choice] = &[
    choice(0, "top-left"),
    choice(1, "top-right"),
    choice(2, "bottom-left"),
    choice(3, "bottom-right"),
];
const TOP_BOTTOM: &[Choice] = &[choice(0, "top"), choice(1, "bottom")];
const TIMER_MODES: &[Choice] = &[
    choice(0, "off"),
    choice(1, "count-up"),
    choice(2, "count-down"),
];
const TIMER_STATES: &[Choice] = &[choice(0, "paused"), choice(1, "running")];
const OVERLAY_LOCATIONS: &[Choice] = &[
    choice(0x0000, "left-top"),
    choice(0x0001, "left-center"),
    choice(0x0002, "left-bottom"),
    choice(0x0010, "right-top"),
    choice(0x0011, "right-center"),
    choice(0x0012, "right-bottom"),
];
const PICTURE_MODES: &[Choice] = &[
    choice(0, "standard"),
    choice(1, "fps"),
    choice(2, "moba"),
    choice(3, "rpg"),
    choice(4, "racing"),
    choice(5, "movie"),
    choice(6, "reader"),
    choice(7, "srgb"),
    choice(8, "custom"),
    choice(9, "eco"),
];
const AUDIO_INPUTS: &[Choice] = &[choice(0, "main"), choice(1, "pip-pbp"), choice(2, "auto")];
const OSD_TRANSPARENCY: &[Choice] = &[
    choice(0, "10%"),
    choice(1, "20%"),
    choice(2, "40%"),
    choice(3, "60%"),
    choice(4, "80%"),
];
const OSD_TIMES: &[Choice] = &[
    choice(5, "5s"),
    choice(10, "10s"),
    choice(15, "15s"),
    choice(20, "20s"),
    choice(25, "25s"),
    choice(30, "30s"),
];
const LED_MODES: &[Choice] = &[choice(0, "on"), choice(1, "off"), choice(2, "standby-on")];
const QUICK_ACTIONS: &[Choice] = &[
    choice(1, "black-equalizer"),
    choice(10, "crosshair"),
    choice(3, "volume"),
    choice(4, "input"),
    choice(5, "contrast"),
    choice(6, "brightness"),
    choice(7, "picture-mode"),
];
const CROSSHAIRS: &[Choice] = &[
    choice(0, "off"),
    choice(1, "style-1"),
    choice(2, "style-2"),
    choice(3, "style-3"),
    choice(4, "custom"),
];
const KVM_DEVICES: &[Choice] = &[choice(0, "usb-b"), choice(1, "usb-c")];
const PIXEL_CLEAN_STATES: &[Choice] = &[choice(0, "off"), choice(1, "on")];

impl ControlName {
    fn spec(self) -> ControlSpec {
        let continuous = |name, address, min, max, description| ControlSpec {
            name,
            address,
            min,
            max,
            readable: true,
            writable: true,
            relative: true,
            verify: true,
            choices: &[],
            description,
        };
        let discrete = |name, address, min, max, choices, description| ControlSpec {
            name,
            address,
            min,
            max,
            readable: true,
            writable: true,
            relative: false,
            verify: true,
            choices,
            description,
        };
        let action = |name, address, description| ControlSpec {
            name,
            address,
            min: 1,
            max: 1,
            readable: false,
            writable: true,
            relative: false,
            verify: false,
            choices: &[],
            description,
        };
        let read_only = |name, address, choices, description| ControlSpec {
            name,
            address,
            min: 0,
            max: u16::MAX,
            readable: true,
            writable: false,
            relative: false,
            verify: false,
            choices,
            description,
        };
        let write_only = |name, address, min, max, description| ControlSpec {
            name,
            address,
            min,
            max,
            readable: false,
            writable: true,
            relative: false,
            verify: false,
            choices: &[],
            description,
        };

        match self {
            Self::Brightness => continuous(
                "brightness",
                Address::Standard(0x10),
                0,
                100,
                "Panel brightness",
            ),
            Self::Contrast => continuous(
                "contrast",
                Address::Standard(0x12),
                0,
                100,
                "Panel contrast",
            ),
            Self::Volume => continuous("volume", Address::Standard(0x62), 0, 100, "Monitor volume"),
            Self::Sharpness => continuous(
                "sharpness",
                Address::Standard(0x87),
                0,
                10,
                "Gigabyte sharpness (0x87 is audio treble in generic MCCS)",
            ),
            Self::BlackEqualizer => continuous(
                "black-equalizer",
                Address::Vendor(0x02),
                0,
                20,
                "Shadow visibility adjustment",
            ),
            Self::ColorTemperature => discrete(
                "color-temperature",
                Address::Vendor(0x03),
                0,
                3,
                COLOR_TEMPERATURES,
                "Color-temperature preset",
            ),
            Self::RedGain => continuous(
                "red-gain",
                Address::Vendor(0x04),
                0,
                100,
                "User color-temperature red gain",
            ),
            Self::BlueGain => continuous(
                "blue-gain",
                Address::Vendor(0x05),
                0,
                100,
                "User color-temperature blue gain",
            ),
            Self::GreenGain => continuous(
                "green-gain",
                Address::Vendor(0x06),
                0,
                100,
                "User color-temperature green gain",
            ),
            Self::Gamma => discrete("gamma", Address::Vendor(0x07), 0, 5, GAMMA, "Gamma preset"),
            Self::ColorVibrance => continuous(
                "color-vibrance",
                Address::Vendor(0x08),
                0,
                20,
                "Color vibrance",
            ),
            Self::UltraClear => discrete(
                "ultra-clear",
                Address::Vendor(0x0A),
                0,
                1,
                ON_OFF,
                "Motion-blur reduction; historically called Aim Stabilizer",
            ),
            Self::LowBlueLight => continuous(
                "low-blue-light",
                Address::Vendor(0x0B),
                0,
                10,
                "Blue-light reduction",
            ),
            Self::Vrr => discrete(
                "vrr",
                Address::Vendor(0x0C),
                0,
                1,
                ON_OFF,
                "Adaptive-Sync / VRR",
            ),
            Self::SuperResolution => continuous(
                "super-resolution",
                Address::Vendor(0x0D),
                0,
                4,
                "Super Resolution level",
            ),
            Self::PipMode => discrete(
                "pip-mode",
                Address::Vendor(0x0E),
                0,
                2,
                PIP_MODES,
                "PIP/PBP mode",
            ),
            Self::PipSource => discrete(
                "pip-source",
                Address::Vendor(0x0F),
                0,
                3,
                INPUTS,
                "Secondary PIP/PBP source",
            ),
            Self::PipSwitch => action(
                "pip-switch",
                Address::Vendor(0x10),
                "Swap the PIP/PBP displays",
            ),
            Self::PipAudioSwitch => action(
                "pip-audio-switch",
                Address::Vendor(0x13),
                "Swap the PIP/PBP audio source",
            ),
            Self::PipSize => discrete(
                "pip-size",
                Address::Vendor(0x14),
                0,
                2,
                PIP_SIZES,
                "PIP window size",
            ),
            Self::PipLocation => discrete(
                "pip-location",
                Address::Vendor(0x15),
                0,
                3,
                PIP_LOCATIONS,
                "PIP window location",
            ),
            Self::Dashboard => discrete(
                "dashboard",
                Address::Vendor(0x18),
                0,
                1,
                ON_OFF,
                "Hardware dashboard overlay",
            ),
            Self::DashboardLocation => discrete(
                "dashboard-location",
                Address::Vendor(0x19),
                0,
                1,
                TOP_BOTTOM,
                "Dashboard overlay location",
            ),
            Self::RefreshRateOverlay => discrete(
                "refresh-rate-overlay",
                Address::Vendor(0x22),
                0,
                1,
                ON_OFF,
                "Refresh-rate/FPS overlay",
            ),
            Self::TimerMode => discrete(
                "timer-mode",
                Address::Vendor(0x23),
                0,
                2,
                TIMER_MODES,
                "Game timer mode",
            ),
            Self::TimerCurrent => read_only(
                "timer-current",
                Address::Vendor(0x25),
                &[],
                "Current game timer value",
            ),
            Self::TimerValue => write_only(
                "timer-value",
                Address::Vendor(0x26),
                0,
                0x633B,
                "Game timer as MM:SS or packed 0xMMSS",
            ),
            Self::TimerState => discrete(
                "timer-state",
                Address::Vendor(0x27),
                0,
                1,
                TIMER_STATES,
                "Pause or resume the game timer",
            ),
            Self::Counter => discrete(
                "counter",
                Address::Vendor(0x28),
                0,
                1,
                ON_OFF,
                "Game counter overlay",
            ),
            Self::CounterCurrent => read_only(
                "counter-current",
                Address::Vendor(0x29),
                &[],
                "Current game counter value",
            ),
            Self::CounterValue => write_only(
                "counter-value",
                Address::Vendor(0x2A),
                0,
                99,
                "Game counter value",
            ),
            Self::OverlayLocation => discrete(
                "overlay-location",
                Address::Vendor(0x2B),
                0,
                0x0012,
                OVERLAY_LOCATIONS,
                "Timer/counter/refresh-rate overlay location",
            ),
            Self::PictureMode => discrete(
                "picture-mode",
                Address::Vendor(0x2C),
                0,
                9,
                PICTURE_MODES,
                "MO27Q28G picture mode",
            ),
            Self::Input => discrete(
                "input",
                Address::Vendor(0x2D),
                0,
                3,
                INPUTS,
                "Video input source",
            ),
            Self::AudioInput => discrete(
                "audio-input",
                Address::Vendor(0x2E),
                0,
                2,
                AUDIO_INPUTS,
                "Audio source in PIP/PBP",
            ),
            Self::OsdTransparency => discrete(
                "osd-transparency",
                Address::Vendor(0x2F),
                0,
                4,
                OSD_TRANSPARENCY,
                "MO27Q28G OSD opacity",
            ),
            Self::OsdTime => discrete(
                "osd-time",
                Address::Vendor(0x30),
                5,
                30,
                OSD_TIMES,
                "OSD display duration",
            ),
            Self::LedIndicator => discrete(
                "led-indicator",
                Address::Vendor(0x31),
                0,
                2,
                LED_MODES,
                "Power LED behavior",
            ),
            Self::QuickUp => discrete(
                "quick-up",
                Address::Vendor(0x32),
                0,
                10,
                QUICK_ACTIONS,
                "Joystick up shortcut",
            ),
            Self::QuickDown => discrete(
                "quick-down",
                Address::Vendor(0x33),
                0,
                10,
                QUICK_ACTIONS,
                "Joystick down shortcut",
            ),
            Self::QuickRight => discrete(
                "quick-right",
                Address::Vendor(0x34),
                0,
                10,
                QUICK_ACTIONS,
                "Joystick right shortcut",
            ),
            Self::QuickLeft => discrete(
                "quick-left",
                Address::Vendor(0x35),
                0,
                10,
                QUICK_ACTIONS,
                "Joystick left shortcut",
            ),
            Self::Crosshair => discrete(
                "crosshair",
                Address::Vendor(0x37),
                0,
                4,
                CROSSHAIRS,
                "Hardware crosshair",
            ),
            Self::FirmwareVersion => read_only(
                "firmware-version",
                Address::Vendor(0x38),
                &[],
                "Firmware version reported by the scaler",
            ),
            Self::Hdr => read_only("hdr", Address::Vendor(0x53), ON_OFF, "Current HDR state"),
            Self::KvmDevice => ControlSpec {
                verify: false,
                ..discrete(
                    "kvm-device",
                    Address::Vendor(0x69),
                    0,
                    1,
                    KVM_DEVICES,
                    "Switch the active USB upstream host",
                )
            },
            Self::KvmButton => discrete(
                "kvm-button",
                Address::Vendor(0x6A),
                0,
                1,
                ON_OFF,
                "Enable the monitor KVM switch button",
            ),
            Self::KvmUsbBInput => discrete(
                "kvm-usb-b-input",
                Address::Vendor(0x6B),
                0,
                3,
                INPUTS,
                "Video input associated with USB-B",
            ),
            Self::KvmUsbCInput => discrete(
                "kvm-usb-c-input",
                Address::Vendor(0x6C),
                0,
                3,
                INPUTS,
                "Video input associated with USB-C",
            ),
            Self::PanelHours => read_only(
                "panel-hours",
                Address::Vendor(0x7F),
                &[],
                "OLED panel usage time",
            ),
            Self::PixelCleanState => read_only(
                "pixel-clean-state",
                Address::Vendor(0x80),
                PIXEL_CLEAN_STATES,
                "Pixel Clean state",
            ),
            Self::PixelCleanCount => read_only(
                "pixel-clean-count",
                Address::Vendor(0x81),
                &[],
                "Completed Pixel Clean count",
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Response {
    maximum: u16,
    current: u16,
    data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestedValue {
    Absolute(u16),
    Relative(i32),
}

pub fn run(args: GigabyteArgs) -> Result<()> {
    match args.command {
        GigabyteCommand::Devices => list_devices(),
        GigabyteCommand::Controls => {
            list_controls();
            Ok(())
        }
        command => {
            ensure_supported_monitor(args.force)?;
            let monitor = GigabyteMonitor::open(args.device)?;
            match command {
                GigabyteCommand::Status => show_status(&monitor),
                GigabyteCommand::Get { control } => show_control(&monitor, control),
                GigabyteCommand::Set { control, value } => set_control(&monitor, control, &value),
                GigabyteCommand::Oled { command } => run_oled(&monitor, command),
                GigabyteCommand::Devices | GigabyteCommand::Controls => unreachable!(),
            }
        }
    }
}

fn ensure_supported_monitor(force: bool) -> Result<()> {
    if force || supported_monitor_present() {
        return Ok(());
    }

    bail!(
        "refusing Gigabyte operations because an attached {MO27Q28G_MODEL} ({MO27Q28G_EDID_ID}) was not confirmed; use --force only if this is the intended Realtek-based Gigabyte monitor"
    )
}

fn supported_monitor_present() -> bool {
    Display::enumerate().into_iter().any(|display| {
        let info = display.info;
        info.id.contains(MO27Q28G_EDID_ID)
            || info.id.to_ascii_uppercase().contains(MO27Q28G_MODEL)
            || info
                .model_name
                .as_deref()
                .is_some_and(|model| model.eq_ignore_ascii_case(MO27Q28G_MODEL))
    })
}

fn list_devices() -> Result<()> {
    let api = HidApi::new().context("failed to initialize HID access")?;
    let devices: Vec<_> = matching_devices(&api).collect();
    if devices.is_empty() {
        bail!(
            "no Gigabyte/Realtek HID device {:04X}:{:04X} found; connect the monitor USB upstream cable",
            GIGABYTE_HID_VENDOR_ID,
            GIGABYTE_HID_PRODUCT_ID
        );
    }

    for (index, info) in devices.into_iter().enumerate() {
        println!(
            "[{index}] {:04X}:{:04X}",
            info.vendor_id(),
            info.product_id()
        );
        println!("  path: {}", info.path().to_string_lossy());
        println!("  product: {}", info.product_string().unwrap_or("unknown"));
        println!(
            "  manufacturer: {}",
            info.manufacturer_string().unwrap_or("unknown")
        );
        println!(
            "  usage page: 0x{:04X}, usage: 0x{:04X}, interface: {}",
            info.usage_page(),
            info.usage(),
            info.interface_number()
        );
    }
    Ok(())
}

fn matching_devices(api: &HidApi) -> impl Iterator<Item = &DeviceInfo> {
    api.device_list().filter(|info| {
        info.vendor_id() == GIGABYTE_HID_VENDOR_ID && info.product_id() == GIGABYTE_HID_PRODUCT_ID
    })
}

fn list_controls() {
    for &control in ALL_CONTROLS {
        let spec = control.spec();
        let access = match (spec.readable, spec.writable) {
            (true, true) => "read/write",
            (true, false) => "read-only",
            (false, true) => "write-only",
            (false, false) => "unavailable",
        };
        let values = accepted_values(spec);
        println!("{:<22} {:<30} {access}", spec.name, spec.address);
        println!("  {}", spec.description);
        println!("  values: {values}");
    }
}

fn accepted_values(spec: ControlSpec) -> String {
    if !spec.choices.is_empty() {
        return spec
            .choices
            .iter()
            .map(|item| format!("{}={}", item.name, item.value))
            .collect::<Vec<_>>()
            .join(", ");
    }

    if spec.writable {
        let relative = if spec.relative { ", or +N/-N" } else { "" };
        format!("{}..={}{}", spec.min, spec.max, relative)
    } else {
        "reported by monitor".to_owned()
    }
}

fn show_status(monitor: &GigabyteMonitor) -> Result<()> {
    println!("Gigabyte {MO27Q28G_MODEL} via USB HID");
    let controls = [
        ControlName::FirmwareVersion,
        ControlName::Hdr,
        ControlName::PictureMode,
        ControlName::Input,
        ControlName::Brightness,
        ControlName::Contrast,
        ControlName::BlackEqualizer,
        ControlName::Gamma,
        ControlName::Vrr,
        ControlName::KvmDevice,
        ControlName::PanelHours,
        ControlName::PixelCleanState,
        ControlName::PixelCleanCount,
    ];

    let mut successes = 0;
    for control in controls {
        let spec = control.spec();
        match monitor.get(spec.address) {
            Ok(response) => {
                successes += 1;
                println!("  {:<20} {}", spec.name, format_response(spec, &response));
            }
            Err(error) => println!("  {:<20} unavailable ({error:#})", spec.name),
        }
    }

    if successes == 0 {
        bail!("the HID endpoint opened, but no monitor controls returned a valid response");
    }
    Ok(())
}

fn show_control(monitor: &GigabyteMonitor, control: ControlName) -> Result<()> {
    let spec = control.spec();
    if !spec.readable {
        bail!("{} is write-only", spec.name);
    }
    let response = monitor
        .get(spec.address)
        .with_context(|| format!("failed to read {} ({})", spec.name, spec.address))?;
    println!(
        "{}: {} (raw {}, maximum {})",
        spec.name,
        format_response(spec, &response),
        response.current,
        response.maximum
    );
    Ok(())
}

fn set_control(monitor: &GigabyteMonitor, control: ControlName, input: &str) -> Result<()> {
    let spec = control.spec();
    if !spec.writable {
        bail!("{} is read-only", spec.name);
    }

    let requested = parse_requested_value(spec, input)?;
    let (target, previous) = match requested {
        RequestedValue::Absolute(value) => (value, None),
        RequestedValue::Relative(delta) => {
            if !spec.relative || !spec.readable {
                bail!("{} does not accept relative values", spec.name);
            }
            let current = monitor.get(spec.address).with_context(|| {
                format!("failed to read {} before applying {delta:+}", spec.name)
            })?;
            let target = (i32::from(current.current) + delta)
                .clamp(i32::from(spec.min), i32::from(spec.max)) as u16;
            (target, Some(current.current))
        }
    };

    validate_range(spec, target)?;
    monitor
        .set(spec.address, encode_value(spec, target))
        .with_context(|| format!("failed to set {} ({})", spec.name, spec.address))?;

    if spec.verify {
        thread::sleep(IO_DELAY);
        let actual = monitor
            .get(spec.address)
            .with_context(|| format!("{} was sent, but verification failed", spec.name))?;
        if actual.current != target {
            bail!(
                "{} write was not accepted: requested {}, monitor reports {} (the current picture/HDR mode may lock this control)",
                spec.name,
                target,
                actual.current
            );
        }
    }

    match previous {
        Some(old) => println!(
            "{}: {} -> {}",
            spec.name,
            format_value(spec, old),
            format_value(spec, target)
        ),
        None => println!("{}: set to {}", spec.name, format_value(spec, target)),
    }
    Ok(())
}

fn run_oled(monitor: &GigabyteMonitor, command: OledCommand) -> Result<()> {
    match command {
        OledCommand::Status => {
            for control in [
                ControlName::PanelHours,
                ControlName::PixelCleanState,
                ControlName::PixelCleanCount,
            ] {
                show_control(monitor, control)?;
            }
            Ok(())
        }
        OledCommand::PixelClean { command } => match command {
            PixelCleanCommand::Start { yes } => {
                if !yes {
                    bail!(
                        "Pixel Clean blanks the display and can take several minutes; rerun with `oled pixel-clean start --yes` to confirm"
                    );
                }
                monitor
                    .set(Address::Vendor(0x80), 1)
                    .context("failed to start Pixel Clean")?;
                println!(
                    "Pixel Clean start request sent; leave the monitor powered and undisturbed"
                );
                Ok(())
            }
            PixelCleanCommand::Stop => {
                monitor
                    .set(Address::Vendor(0x80), 0)
                    .context("failed to stop Pixel Clean")?;
                println!("Pixel Clean stop request sent");
                Ok(())
            }
        },
    }
}

fn parse_requested_value(spec: ControlSpec, input: &str) -> Result<RequestedValue> {
    if let Some(rest) = input.strip_prefix('+') {
        return Ok(RequestedValue::Relative(parse_u16(rest)? as i32));
    }
    if let Some(rest) = input.strip_prefix('-') {
        return Ok(RequestedValue::Relative(-(parse_u16(rest)? as i32)));
    }

    if spec.address == Address::Vendor(0x26) && input.contains(':') {
        return parse_timer(input).map(RequestedValue::Absolute);
    }

    let normalized = normalize(input);
    if let Some(item) = spec
        .choices
        .iter()
        .find(|item| normalize(item.name) == normalized)
    {
        return Ok(RequestedValue::Absolute(item.value));
    }

    parse_u16(input)
        .map(RequestedValue::Absolute)
        .with_context(|| {
            if spec.choices.is_empty() {
                format!(
                    "invalid value for {}; expected {}",
                    spec.name,
                    accepted_values(spec)
                )
            } else {
                format!(
                    "invalid value `{input}` for {}; expected {}",
                    spec.name,
                    accepted_values(spec)
                )
            }
        })
}

fn parse_timer(input: &str) -> Result<u16> {
    let (minutes, seconds) = input
        .split_once(':')
        .ok_or_else(|| anyhow!("timer value must use MM:SS"))?;
    let minutes: u8 = minutes.parse().context("invalid timer minutes")?;
    let seconds: u8 = seconds.parse().context("invalid timer seconds")?;
    if minutes > 99 || seconds > 59 {
        bail!("timer must be between 00:00 and 99:59");
    }
    Ok(u16::from_be_bytes([minutes, seconds]))
}

fn parse_u16(input: &str) -> Result<u16> {
    if input.is_empty() {
        bail!("expected a number after the sign");
    }
    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u16::from_str_radix(hex, 16).with_context(|| format!("invalid hexadecimal value `{input}`"))
    } else {
        input
            .parse::<u16>()
            .with_context(|| format!("invalid decimal value `{input}`"))
    }
}

fn normalize(input: &str) -> String {
    input
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn validate_range(spec: ControlSpec, value: u16) -> Result<()> {
    if value < spec.min || value > spec.max {
        bail!(
            "{} value {} is outside the safe range {}..={}",
            spec.name,
            value,
            spec.min,
            spec.max
        );
    }
    if !spec.choices.is_empty() && !spec.choices.iter().any(|item| item.value == value) {
        bail!(
            "{} does not recognize value {}; expected {}",
            spec.name,
            value,
            accepted_values(spec)
        );
    }
    Ok(())
}

fn encode_value(spec: ControlSpec, value: u16) -> u16 {
    // The scaler exposes OSD duration as seconds when reading, but its setter
    // accepts a zero-based index for 5, 10, 15, 20, 25, and 30 seconds.
    if spec.address == Address::Vendor(0x30) {
        (value / 5).saturating_sub(1)
    } else if spec.address == Address::Vendor(0x2B) && value >= 0x10 {
        0x0100 | (value & 0x000F)
    } else {
        value
    }
}

fn format_value(spec: ControlSpec, value: u16) -> String {
    if let Some(item) = spec.choices.iter().find(|item| item.value == value) {
        return item.name.to_owned();
    }
    if matches!(spec.address, Address::Vendor(0x25 | 0x26)) {
        let [minutes, seconds] = value.to_be_bytes();
        return format!("{minutes:02}:{seconds:02}");
    }
    if spec.address == Address::Vendor(0x38) {
        return format!("F{value:02}");
    }
    if spec.address == Address::Vendor(0x7F) {
        return format!("{value} hours");
    }
    value.to_string()
}

fn format_response(spec: ControlSpec, response: &Response) -> String {
    if spec.address == Address::Vendor(0x38) && !response.data.is_empty() {
        return format!("{:02X?}", response.data);
    }
    format_value(spec, response.current)
}

struct GigabyteMonitor {
    _api: HidApi,
    device: HidDevice,
}

impl GigabyteMonitor {
    fn open(index: usize) -> Result<Self> {
        let api = HidApi::new().context("failed to initialize HID access")?;
        let info = matching_devices(&api).nth(index).ok_or_else(|| {
            anyhow!(
                "Gigabyte HID device index {index} was not found; run `gigabyte devices` and check the USB upstream cable"
            )
        })?;
        let device = info
            .open_device(&api)
            .with_context(|| format!("failed to open HID device index {index}"))?;
        Ok(Self { _api: api, device })
    }

    fn get(&self, address: Address) -> Result<Response> {
        let mut last_error = None;
        for attempt in 0..QUERY_ATTEMPTS {
            match self.query_once(address) {
                Ok(response) => return Ok(response),
                Err(error) => last_error = Some(error),
            }
            if attempt + 1 < QUERY_ATTEMPTS {
                thread::sleep(IO_DELAY);
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("HID query failed without an error")))
    }

    fn query_once(&self, address: Address) -> Result<Response> {
        let request = build_get_report(address);
        self.send(&request)
            .context("failed to send the HID query")?;
        self.send(&build_finish_report())
            .context("failed to finalize the HID query")?;
        thread::sleep(IO_DELAY);

        let mut response = [0u8; REPORT_LEN];
        response[0] = 0;
        let length = self
            .device
            .get_input_report(&mut response)
            .context("failed to retrieve the HID input report")?;
        parse_response(&response[..length.min(response.len())], address)
    }

    fn set(&self, address: Address, value: u16) -> Result<()> {
        self.send(&build_set_report(address, value))
            .context("failed to send the HID write")?;
        thread::sleep(IO_DELAY);
        Ok(())
    }

    fn send(&self, report: &[u8; REPORT_LEN]) -> Result<()> {
        self.device
            .send_output_report(report)
            .context("USB HID SET_REPORT failed")
    }
}

fn build_base_report(mode: u8) -> [u8; REPORT_LEN] {
    let mut report = [0u8; REPORT_LEN];
    report[0] = 0;
    report[1] = 0x40;
    report[2] = 0xC6;
    report[7] = mode;
    report[9] = 0x6E;
    report[11] = 0x80;
    report
}

fn build_get_report(address: Address) -> [u8; REPORT_LEN] {
    let mut report = build_base_report(0x24);
    let mut message = Vec::with_capacity(6);
    message.push(0x51);
    match address {
        Address::Standard(code) => {
            message.extend([0x82, 0x01, code]);
        }
        Address::Vendor(selector) => {
            message.extend([0x83, 0x01, 0xE0, selector]);
        }
    }
    message.push(ddc_checksum(&message));
    report[DDC_OFFSET..DDC_OFFSET + message.len()].copy_from_slice(&message);
    report
}

fn build_set_report(address: Address, value: u16) -> [u8; REPORT_LEN] {
    let mut report = build_base_report(0x20);
    let [high, low] = value.to_be_bytes();
    let mut message = Vec::with_capacity(8);
    message.push(0x51);
    match address {
        Address::Standard(code) => {
            message.extend([0x84, 0x03, code, high, low]);
        }
        Address::Vendor(selector) => {
            message.extend([0x85, 0x03, 0xE0, selector, high, low]);
        }
    }
    message.push(ddc_checksum(&message));
    report[DDC_OFFSET..DDC_OFFSET + message.len()].copy_from_slice(&message);
    report
}

fn build_finish_report() -> [u8; REPORT_LEN] {
    let mut report = [0u8; REPORT_LEN];
    report[0] = 0;
    report[1] = 0x40;
    report[2] = 0xD6;
    report[3] = 0x51;
    report[7] = 0x24;
    report[9] = 0x6E;
    report[10] = 0x01;
    report[11] = 0x80;
    report
}

fn ddc_checksum(message: &[u8]) -> u8 {
    message
        .iter()
        .copied()
        .fold(0, |checksum, byte| checksum ^ byte)
}

fn parse_response(report: &[u8], address: Address) -> Result<Response> {
    let (expected_code, expected_selector) = match address {
        Address::Standard(code) => (code, None),
        Address::Vendor(selector) => (0xE0, Some(selector)),
    };

    for start in 0..report.len().saturating_sub(10) {
        let prefix = &report[start..];
        if prefix[0] != 0x6E || prefix[2] != 0x02 || prefix[4] != expected_code {
            continue;
        }
        let total_length = usize::from(prefix[1] & 0x7F) + 3;
        if prefix.len() < total_length || total_length < 11 {
            continue;
        }
        let candidate = &prefix[..total_length];
        if let Some(selector) = expected_selector {
            if candidate[5] != selector {
                continue;
            }
        }
        if candidate[3] != 0 {
            bail!("monitor returned DDC/CI result code 0x{:02X}", candidate[3]);
        }
        // DDC/CI monitor replies checksum against the virtual host address 0x50,
        // which is not included in the bytes returned by this HID wrapper.
        let checksum = ddc_checksum(candidate);
        if checksum != 0x50 {
            bail!(
                "monitor returned a response with checksum residue 0x{checksum:02X}: {:02X?}",
                candidate
            );
        }
        return Ok(Response {
            maximum: u16::from_be_bytes([candidate[6], candidate[7]]),
            current: u16::from_be_bytes([candidate[8], candidate[9]]),
            data: candidate[10..candidate.len() - 1].to_vec(),
        });
    }

    bail!(
        "the HID input report did not contain the expected {} response",
        address
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_known_standard_write_frame() {
        let report = build_set_report(Address::Standard(0x12), 49);
        assert_eq!(
            &report[DDC_OFFSET..DDC_OFFSET + 8],
            &[0x51, 0x84, 0x03, 0x12, 0x00, 0x31, 0xF5, 0x00]
        );
    }

    #[test]
    fn builds_known_vendor_read_frame() {
        let report = build_get_report(Address::Vendor(0x2C));
        assert_eq!(
            &report[DDC_OFFSET..DDC_OFFSET + 8],
            &[0x51, 0x83, 0x01, 0xE0, 0x2C, 0x1F, 0x00, 0x00]
        );
    }

    #[test]
    fn parses_known_vendor_response() {
        let report = [
            0x6E, 0x88, 0x02, 0x00, 0xE0, 0x2C, 0x00, 0x09, 0x00, 0x00, 0x71,
        ];
        assert_eq!(
            parse_response(&report, Address::Vendor(0x2C)).unwrap(),
            Response {
                maximum: 9,
                current: 0,
                data: vec![]
            }
        );
    }

    #[test]
    fn parses_symbolic_and_relative_values() {
        let picture = ControlName::PictureMode.spec();
        assert_eq!(
            parse_requested_value(picture, "sRGB").unwrap(),
            RequestedValue::Absolute(7)
        );
        let brightness = ControlName::Brightness.spec();
        assert_eq!(
            parse_requested_value(brightness, "-10").unwrap(),
            RequestedValue::Relative(-10)
        );
    }

    #[test]
    fn parses_timer_values() {
        let timer = ControlName::TimerValue.spec();
        assert_eq!(
            parse_requested_value(timer, "12:34").unwrap(),
            RequestedValue::Absolute(0x0C22)
        );
    }

    #[test]
    fn mo27q28g_quick_actions_use_command_values_not_ui_indexes() {
        let quick = ControlName::QuickUp.spec();
        assert_eq!(
            parse_requested_value(quick, "crosshair").unwrap(),
            RequestedValue::Absolute(10)
        );
        assert_eq!(
            parse_requested_value(quick, "picture-mode").unwrap(),
            RequestedValue::Absolute(7)
        );
    }

    #[test]
    fn osd_time_uses_an_index_for_writes() {
        let spec = ControlName::OsdTime.spec();
        assert_eq!(encode_value(spec, 5), 0);
        assert_eq!(encode_value(spec, 15), 2);
        assert_eq!(encode_value(spec, 30), 5);
    }

    #[test]
    fn right_side_overlay_locations_use_a_high_byte_for_writes() {
        let spec = ControlName::OverlayLocation.spec();
        assert_eq!(encode_value(spec, 0x00), 0x0000);
        assert_eq!(encode_value(spec, 0x02), 0x0002);
        assert_eq!(encode_value(spec, 0x10), 0x0100);
        assert_eq!(encode_value(spec, 0x12), 0x0102);
    }
}
