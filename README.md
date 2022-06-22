# mmc
Minimalist Monitor Control - a minimalist CLI app written in Rust to read and set VCP features for your monitor via the DDC/CI protocol.

## What are VCP / VCP Features ?

VCP stands for Virtual Control Panel. VCP features basically are monitor settings, and allow you to control bightness, contrast, etc. of your screen. This program uses DDC/CI (Display Data Channel/Command Interface) to talk to the monitor and change those settings.

## How to compile and run

Just clone the repository, then run `cargo run`, or `cargo build --release` to build a release binary.

Help:

```
mmc 1.2.0
Hexalyse
Minimalist Monitor Control

USAGE:
    mmc.exe [OPTIONS] -i <VCP_ID> <--get|--set>

OPTIONS:
    -b <BACKEND>          Only act on monitors using this backend [possible values: winapi, nvapi,
                          i2c, macos]
    -g, --get             Get VCP value
    -h, --help            Print help information
    -i <VCP_ID>           The VCP identifier (eg: 10 for brightness)
    -s, --set             Set VCP value
    -u                    Force update the capabilities before reading or writing the VCP value
                          (needed on some screens?)
    -v <VCP_VALUE>        The VCP value (only used with '-s/--set')
    -V, --version         Print version information
```

## Alternatives

This program essentially does what [ddcset-rs](https://github.com/arcnmx/ddcset-rs) does, except `ddcset-rs` didn't work on my hardware : for some reason my drivers sometimes fail to communicate with the monitor, and I get I2C communication errors. Since the problem is totally random, I fixed it in `mmc` by retrying until it works. It's ugly, but it works.
