#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::flash::Blocking;
use embassy_stm32::flash::Flash;
use embassy_stm32::gpio::{Input, Pull};
use embassy_time::{Duration, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

#[repr(C)]
#[derive(Clone, Copy)]
struct PersistentData {
    counter: u32,
    checksum: u32,
    reserved: [u8; 24], // Padding for 32 bytes size
}
const _: () = assert!(core::mem::size_of::<PersistentData>() == 32);

impl PersistentData {
    fn new(counter: u32) -> Self {
        Self {
            counter,
            checksum: Self::calculate_checksum(counter),
            reserved: Default::default(),
        }
    }

    fn is_valid(&self) -> bool {
        self.checksum == Self::calculate_checksum(self.counter)
    }

    fn calculate_checksum(value: u32) -> u32 {
        value ^ 0xDEADBEEF
    }
}

unsafe extern "C" {
    static _storage_start: u32;
    static _storage_end: u32;
    static _flash_start: u32;
}

fn flash_start() -> u32 {
    unsafe { &_flash_start as *const u32 as u32 }
}

fn storage_addr() -> u32 {
    let absolut_addr = unsafe { &_storage_start as *const u32 as u32 };
    absolut_addr - flash_start()
}

fn storage_end() -> u32 {
    let absolut_addr = unsafe { &_storage_end as *const u32 as u32 };
    absolut_addr - flash_start()
}

fn storage_size() -> u32 {
    storage_end() - storage_addr()
}

static FLASH: StaticCell<Flash<'static, Blocking>> = StaticCell::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Config::default());
    let power_good = Input::new(p.PA0, Pull::Up);

    info!("storage start: {=u32:x}", storage_addr());
    info!("storage end: {=u32:x}", storage_end());
    info!("storage size: {}", storage_size());

    let flash = Flash::new_blocking(p.FLASH);
    let mut flash = FLASH.init(flash);

    let mut counter = load_counter(&mut flash);

    loop {
        Timer::after(Duration::from_secs(1)).await;

        counter += 1;
        info!("counter: {}", counter);

        if power_good.is_low() {
            save_counter(&mut flash, counter);

            loop {
                cortex_m::asm::nop();
            }
        }
    }
}

fn load_counter(flash: &mut Flash<'static, Blocking>) -> u32 {
    let mut buf = [0u8; size_of::<PersistentData>()];

    flash
        .read(storage_addr(), &mut buf)
        .expect("Failed to read flash");

    let data: PersistentData =
        unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const PersistentData) };

    if data.is_valid() { data.counter } else { 0 }
}

fn save_counter(flash: &mut Flash<'static, Blocking>, counter: u32) {
    let data = PersistentData::new(counter);

    let bytes = unsafe {
        core::slice::from_raw_parts(
            &data as *const PersistentData as *const u8,
            size_of::<PersistentData>(),
        )
    };

    flash
        .erase(storage_addr(), storage_end())
        .expect("Failed to erase flash");

    flash
        .write(storage_addr(), bytes)
        .expect("Failed to write to flash");
}
