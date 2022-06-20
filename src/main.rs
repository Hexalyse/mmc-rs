use ddc_hi::{Ddc, Display};
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
        display.update_capabilities().ok();
        if !quiet {
            println!("Display detected: {:?} {}: {:?} {:?}",
                display.info.backend, display.info.id,
                display.info.manufacturer_id, display.info.model_name
            );
        }
        if display.info.manufacturer_id != None {
            if !quiet {
                let value = display.handle.get_vcp_feature(0x10).unwrap();
                println!("Previous brightness: {:?}", value.value());
            }
            display.handle.set_vcp_feature(0x10, brightness).unwrap();
            if !quiet {
                let value = display.handle.get_vcp_feature(0x10).unwrap();
                println!("New brightness: {:?}", value.value());
            }
        } else if !quiet {
            println!("Doing nothing because the display has no Manufacturer");
        }
    }
    Ok(())
}