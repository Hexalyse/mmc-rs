# mmc
Minimalist Monitor Control - a minimalist CLI app written in Rust to read and set VCP values for your monitor via the DDC protocol.

## What are VCP / VCP Features ?

VCP stands for Virtual Control Panel. VCP features basically are monitor settings, and allow you to control bightness, contrast, etc. of your screen. This program uses DDC/CI (Display Data Channel/Command Interface) to talk to the monitor and change those settings.

## How to compile and run

Just clone the repository, then run `cargo run`, or `cargo build --release` to build a release binary.

## Alternatives

This program essentially does what [ddcset-rs](https://github.com/arcnmx/ddcset-rs) does, except `ddcset-rs` didn't work on my hardware : for some reason my drivers sometimes fail to communicate with the monitor sometimes, and I get I2C communication errors. Since the problem is totally random, I fixed it in `mmc` by retrying until it works. It's ugly, but it works.
