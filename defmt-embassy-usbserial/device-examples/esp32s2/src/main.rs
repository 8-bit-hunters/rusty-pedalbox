#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::otg_fs::{Usb, asynch::Driver};
use esp_hal::peripherals::{GPIO19, GPIO20, USB0};
use esp_hal::timer::timg::TimerGroup;
use static_cell::ConstStaticCell;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

static EP_OUT_BUFFER: ConstStaticCell<[u8; 128]> = ConstStaticCell::new([0u8; 128]);

#[embassy_executor::task]
async fn defmtusb_wrapper(usb0: USB0<'static>, dp: GPIO20<'static>, dm: GPIO19<'static>) {
    let usb_peri = Usb::new(usb0, dp, dm);
    let usb_driver = Driver::new(usb_peri, EP_OUT_BUFFER.take(), Default::default());
    let config = {
        let mut c = embassy_usb::Config::new(0x1234, 0x5678);
        c.serial_number = Some("defmt");
        c.max_packet_size_0 = 64;
        c.composite_with_iads = true;
        c.device_class = 0xEF;
        c.device_sub_class = 0x02;
        c.device_protocol = 0x01;
        c
    };
    defmt_embassy_usbserial::run(usb_driver, config).await;
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 0.6.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);

    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0);

    spawner.must_spawn(defmtusb_wrapper(p.USB0, p.GPIO20, p.GPIO19));

    defmt::info!("Starting loop.");
    loop {
        defmt::info!("Hello world!  {=u64:tms}", Instant::now().as_millis());
        Timer::after(Duration::from_millis(100)).await;
    }
}
