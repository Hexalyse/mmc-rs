use anyhow;
use clap::ArgGroup;
use clap::Parser;
use ddc_hi::{Backend, Ddc, Display, VcpValue};
use retry::delay::Fixed;
use retry::retry;

#[derive(Parser, Debug)]
#[clap(author = "Hexalyse", version, about = "Minimalist Monitor Control")]
#[structopt(group = ArgGroup::with_name("action").required(true))]
struct CliArguments {
    /// Get VCP value
    #[clap(short, long, group = "action")]
    get: bool,
    /// Set VCP value
    #[clap(short, long, group = "action")]
    set: bool,
    /// The VCP identifier (eg: 10 for brightness)
    #[clap(short = 'i')]
    vcp_id: String,
    /// The VCP value (only used with '-s/--set')
    #[clap(short)]
    vcp_value: Option<u16>,
    /// Force update the capabilities before reading or writing the VCP value (needed on some screens?)
    #[clap(short, takes_value = false)]
    update_capabilities: bool,
    /// Only act on monitors using this backend [possible values: winapi, nvapi, i2c, macos]
    #[clap(short)]
    backend: Option<String>,
    /// Add the value to the current value
    #[clap(long)]
    add: bool,
    /// Subtract the value from the current value
    #[clap(long)]
    subtract: bool,
}

fn get_backend(backend: Option<&str>) -> Backend {
    match backend {
        Some("winapi") => Backend::WinApi,
        Some("nvapi") => Backend::Nvapi,
        Some("i2c") => Backend::I2cDevice,
        Some("macos") => Backend::MacOS,
        None => Backend::Nvapi,
        _ => panic!("Unknown backend: {}", backend.unwrap()),
    }
}

fn set_vcp_with_retry(
    vcp_id: u8,
    vcp_value: u16,
    display: &mut Display,
) -> Result<(), retry::Error<anyhow::Error>> {
    retry(Fixed::from_millis(100).take(20), || {
        display.handle.set_vcp_feature(vcp_id, vcp_value)
    })
}

fn get_vcp_with_retry(
    vcp_id: u8,
    display: &mut Display,
) -> Result<VcpValue, retry::Error<anyhow::Error>> {
    retry(Fixed::from_millis(100).take(20), || {
        display.handle.get_vcp_feature(vcp_id)
    })
}

fn main() -> Result<(), std::io::Error> {
    let args = CliArguments::parse();
    let vcp_id = match u8::from_str_radix(&args.vcp_id, 16) {
        Ok(num) => num,
        Err(_) => panic!("Problem parsing the VCP id."),
    };
    let backend = get_backend(args.backend.as_deref());

    if args.set {
        if args.vcp_value == None {
            println!("Please specify a VCP value.");
            return Ok(());
        }
        let vcp_value = args.vcp_value.unwrap();
        for mut display in Display::enumerate() {
            if !(args.backend == None) && backend != display.info.backend {
                continue;
            }
            if args.update_capabilities {
                display.update_capabilities().ok();
            }
            if args.add || args.subtract {
                let current_value = retry(Fixed::from_millis(100).take(20), || {
                    display.handle.get_vcp_feature(vcp_id)
                });
                let current_value = current_value.unwrap().value();
                let final_value = {
                    if args.add {
                        current_value + vcp_value
                    } else {
                        current_value - vcp_value
                    }
                };
                set_vcp_with_retry(vcp_id, final_value, &mut display).unwrap();
                continue;
            }
            set_vcp_with_retry(vcp_id, vcp_value, &mut display).unwrap();
        }
    } else if args.get {
        for mut display in Display::enumerate() {
            if !(args.backend == None) && backend != display.info.backend {
                continue;
            }
            if args.update_capabilities {
                display.update_capabilities().ok();
            }
            let value = get_vcp_with_retry(vcp_id, &mut display).unwrap();
            println!(
                "[{}] {:?} {:?} - VCP {}: {:?}",
                display.info.backend,
                display.info.manufacturer_id,
                display.info.model_name,
                vcp_id,
                value.value()
            );
        }
    }

    Ok(())
}
