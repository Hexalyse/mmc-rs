# Improvement roadmap

## GIGABYTE MO27Q28G USB HID backend

Status: implemented in version 2.1.

- Discover the Realtek `0BDA:1100` monitor HID endpoint without GCC or OSD Sidekick.
- Gate operations on the MO27Q28G display identity.
- Read and write the verified manufacturer-specific picture, color, gaming-assist, PIP/PBP, KVM, OSD, and Quick Switch controls.
- Read firmware/HDR status, OLED panel hours, Pixel Clean state, and completed-clean count.
- Guard manual Pixel Clean behind explicit confirmation.
- Verify ordinary writes and handle MO27Q28G-specific asymmetric encodings for OSD time and overlay position.
- Keep firmware flashing, unknown OLED-care selectors, dashboard telemetry streaming, and custom-crosshair upload out of the safe backend.

## 1. Safer core and useful CLI

Status: implemented in the current working tree.

- Replace the legacy flag combination with discoverable subcommands.
- Accept absolute values and signed relative changes such as `+10` and `-10`.
- Clamp relative changes to the range reported by the monitor.
- Replace panics and unchecked arithmetic with contextual errors.
- Default to WinAPI on Windows so a future NVIDIA driver does not cause the same display to be modified through both WinAPI and NVAPI.
- Add display filtering, capabilities discovery, common named controls, and arbitrary VCP access.
- Add parser and boundary tests, formatting, Clippy, and release-build validation.

## 2. Rich MCCS values

Priority: next.

- Show symbolic names for discrete values, for example DisplayPort/HDMI input values, power states, mute states, OSD languages, and color presets.
- Accept symbolic values (`input displayport-1`, `power on`) in addition to numbers.
- Correctly distinguish continuous, non-continuous, read-only, write-only, momentary, and table controls.
- Refuse writes to read-only controls and relative changes on non-continuous controls even when using raw VCP mode.
- Add an optional `controls --probe` mode that reads safe controls and reports their current/maximum values.

## 3. Stable multi-monitor targeting

Priority: high before wider distribution.

- Give every detected monitor a stable selector based on EDID manufacturer/model/serial where available.
- Add `--display-index`, exact ID matching, and an explicit `--all` switch.
- Detect duplicate WinAPI/NVAPI handles for the same physical monitor.
- Require an explicit target when more than one display matches a write operation.

## 4. Separate reusable library from CLI

Priority: medium.

- Move value parsing, monitor selection, retries, and DDC operations into `src/lib.rs`.
- Keep `src/main.rs` limited to argument parsing and output.
- Add a mock DDC transport so read/write failures and multi-monitor behavior can be tested without physical hardware.
- Add structured JSON output for scripts and future GUI/tray clients.

## 5. Dependency and backend maintenance

Priority: medium-high.

- Evaluate replacing or maintaining a fork of `ddc-hi`. Version `0.4.1` is still its newest release, but its parser stack currently pulls in `nom 3`, which Rust flags as future-incompatible.
- Build only platform-relevant backends by default to reduce binary size and dependency surface.
- Test WinAPI, NVAPI, Linux I2C, and macOS backends in CI where possible.
- Make retry count and delay configurable for unusually slow monitors.

## 6. Windows ergonomics and distribution

Priority: after the CLI stabilizes.

- Rename the distributed executable to avoid collision with Windows' `mmc.exe`.
- Add shell completions and optional PowerShell helper functions.
- Publish signed GitHub release binaries and checksums from CI.
- Consider a WinGet manifest once releases are stable.
- Build a small optional tray/hotkey frontend on top of the library rather than adding UI code to the CLI.

## 7. Profiles and automation

Priority: optional.

- Save and restore named monitor profiles.
- Apply profiles by time of day, power state, or foreground application.
- Avoid automatic brightness changes during color-critical Lightroom or Photoshop sessions.
- Treat HDR and SDR as separate profiles because many monitors lock or reinterpret luminance controls in HDR.
