# mmc

Minimalist Monitor Control is a small command-line application for reading and changing monitor settings through DDC/CI and the VESA Monitor Control Command Set (MCCS).

It provides friendly commands for common controls while retaining access to every raw VCP feature exposed by a monitor.

## Requirements

- A monitor with DDC/CI support.
- A display connection and graphics driver that pass DDC/CI commands through to the monitor.
- Rust stable when building from source.

DDC/CI changes the monitor's hardware settings. It does not dim the desktop through a software overlay. Some controls can be unavailable or locked while HDR is active.

## Build

```powershell
git clone https://github.com/Hexalyse/mmc-rs.git
cd mmc-rs
cargo build --release
```

The executable is written to `target/release/mmc-rs.exe` on Windows.

> [!NOTE]
> The `mmc-rs` name avoids a conflict with Windows' built-in `C:\Windows\System32\mmc.exe` Microsoft Management Console.

## Usage

Read the current brightness:

```powershell
.\target\release\mmc-rs.exe brightness
```

Set an absolute brightness:

```powershell
.\target\release\mmc-rs.exe brightness 50
```

Increase or decrease brightness relative to its current value:

```powershell
.\target\release\mmc-rs.exe brightness +10
.\target\release\mmc-rs.exe brightness -10
```

Relative changes are clamped to the `0..maximum` range reported by the monitor.

The same absolute/relative syntax works with continuous controls:

```powershell
.\target\release\mmc-rs.exe contrast -5
.\target\release\mmc-rs.exe volume +5
.\target\release\mmc-rs.exe red-gain 50
```

Discover the controls advertised by the monitor:

```powershell
.\target\release\mmc-rs.exe displays
.\target\release\mmc-rs.exe controls
.\target\release\mmc-rs.exe capabilities
```

Use any arbitrary VCP feature code. VCP codes are hexadecimal; values are decimal unless prefixed by `0x`:

```powershell
# Read VCP 0x10
.\target\release\mmc-rs.exe vcp 10

# Select a monitor-specific input value
.\target\release\mmc-rs.exe vcp 60 0x0F
```

Use `--display` to target a monitor by a case-insensitive substring of its ID, model, manufacturer, or serial number:

```powershell
.\target\release\mmc-rs.exe --display MO27Q28G brightness +10
```

On Windows, the WinAPI backend is selected by default. A backend can be selected explicitly:

```powershell
.\target\release\mmc-rs.exe --backend winapi brightness
.\target\release\mmc-rs.exe --backend nvapi brightness
```

## Named controls

| Command | VCP code | Relative values |
|---|---:|:---:|
| `color-temperature` | `0x0C` | Yes |
| `brightness` | `0x10` | Yes |
| `contrast` | `0x12` | Yes |
| `color-preset` | `0x14` | No |
| `red-gain` | `0x16` | Yes |
| `green-gain` | `0x18` | Yes |
| `blue-gain` | `0x1A` | Yes |
| `input` | `0x60` | No |
| `volume` | `0x62` | Yes |
| `treble` | `0x87` | Yes |
| `mute` | `0x8D` | No |
| `osd-language` | `0xCC` | No |
| `power` | `0xD6` | No |

Discrete values are monitor-specific. Run `mmc controls` before changing them. Some power values turn the monitor off and can make DDC/CI temporarily unavailable.

## Reliability

DDC/CI implementations and graphics drivers can be unreliable. Each operation is retried up to 20 times at 100 ms intervals. Errors are reported with the affected display instead of causing a panic.

See [ROADMAP.md](ROADMAP.md) for planned improvements.
