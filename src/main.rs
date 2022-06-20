use ddc_hi::{Ddc, Display, Backend};
use retry::retry;
use retry::delay::Fixed;
use std::env;

fn main() -> Result<(), std::io::Error> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Please provide the brightness value as an argument.");
        return Ok(());
    }
    let brightness: u16 = args[1].parse().unwrap();
    let quiet: bool = args.len() > 2 && String::from("quiet").eq(&args[2]);

    for mut display in Display::enumerate() {
        if display.info.backend != Backend::WinApi {
            continue;
        }
        if !quiet {
            display.update_capabilities().ok();
            println!("Display detected: {:?} {}: {:?} {:?}",
                display.info.backend, display.info.id,
                display.info.manufacturer_id, display.info.model_name
            );
            let value = display.handle.get_vcp_feature(0x10).unwrap();
            println!("Previous brightness: {:?}", value.value());
        };
        let _result = retry(Fixed::from_millis(100).take(10), || {
            display.handle.set_vcp_feature(0x10, brightness)
        });
        if !quiet {
            let value = display.handle.get_vcp_feature(0x10).unwrap();
            println!("New brightness: {:?}", value.value());
        }

    }
    Ok(())
}