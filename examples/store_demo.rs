#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::flash::Flash;
use embassy_stm32::{init, Config};
use embassy_time::{Duration, Timer};
use embedded_storage::nor_flash::ReadNorFlash;
use rusty_pedalbox::calibration::fixed::FixedRange;
use rusty_pedalbox::platform_stm32::FlashStorage;

use defmt_rtt as _;
use panic_probe as _;
use rusty_pedalbox::calibration::{Range, StoredRange};
use rusty_pedalbox::storage::Storage;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = init(Config::default());

    // Create flash driver
    let flash = Flash::new_blocking(p.FLASH);
    defmt::info!("flash capacity = {}", flash.capacity());

    // Define flash storage to save stored range
    const N: usize = size_of::<StoredRange<u16>>();
    let mut storage = FlashStorage::<N>::new(flash);

    // Create a fixed range for storing it
    let value = FixedRange::default().min(123).max(456);

    defmt::info!("Saving...");
    storage.save(&value.to_stored()).expect("save failed");

    defmt::info!("Loading...");
    let loaded: Option<StoredRange<u16>> = storage.load();

    match loaded {
        Some(v) => {
            defmt::info! {"Loaded: min={}, max={}", v.min, v.max};

            assert_eq!(v.min, value.get_min());
            assert_eq!(v.max, value.get_max());
        }
        None => {
            defmt::error!("No valid data found in flash");
        }
    }
}
