use ddc_hi::{Ddc, Display, Backend};
use retry::retry;
use retry::delay::Fixed;
use clap::Parser;
use clap::ArgGroup;

#[derive(Parser, Debug)]
#[clap(author="Hexalyse", version, about="Minimalist Monitor Control")]
#[structopt(group = ArgGroup::with_name("action").required(true))]
struct CliArguments {
    /// Get VCP value
    #[clap(short, long, group="action")]
    get: bool,
    /// Set VCP value
    #[clap(short, long, group="action")]
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
    backend: Option<String>
}

fn get_backend(backend: Option<&str>) -> Backend {
    match backend {
        Some("winapi") => Backend::WinApi,
        Some("nvapi") => Backend::Nvapi,
        Some("i2c") => Backend::I2cDevice,
        Some("macos") => Backend::MacOS,
        None => Backend::Nvapi,
        _ => panic!("Unknown backend: {}", backend.unwrap())
    }
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
            let _result = retry(Fixed::from_millis(100).take(20), || {
                display.handle.set_vcp_feature(vcp_id, vcp_value)
            });
        }
    } else if args.get {
        for mut display in Display::enumerate() {
            if !(args.backend == None) && backend != display.info.backend {
                continue;
            }
            if args.update_capabilities {
                display.update_capabilities().ok();
            }
            let value = retry(Fixed::from_millis(100).take(20), || {
                display.handle.get_vcp_feature(vcp_id)
            });
            println!("[{}] {:?} {:?} - VCP {}: {:?}",
                display.info.backend, display.info.manufacturer_id, display.info.model_name, vcp_id, value);
        }
    }

    Ok(())
}