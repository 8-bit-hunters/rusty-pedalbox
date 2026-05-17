#![no_std]
#![no_main]

use embassy_executor::task;
use embassy_rp::{Peri, bind_interrupts, peripherals::USB};
use embassy_time::Instant;
use embedded_hal_async::delay::DelayNs;
use panic_probe as _;
use rtt_target::{rprintln, rtt_init_print};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<embassy_rp::peripherals::USB>;
});

#[task]
async fn defmtusb_wrapper(usb: Peri<'static, USB>) {
    let driver = embassy_rp::usb::Driver::new(usb, Irqs);
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
    rprintln!("defmtusb::run");
    defmt_embassy_usbserial::run(driver, config).await;
}

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    let p = embassy_rp::init(Default::default());
    rtt_init_print!();
    let mut delay = embassy_time::Delay;

    rprintln!("main :: attempting to spawn usb task");
    spawner.must_spawn(defmtusb_wrapper(p.USB));

    rprintln!("Starting loop");
    defmt::info!("Starting loop");
    loop {
        defmt::info!("Hello, world!  {=u64:tms}", Instant::now().as_millis());
        delay.delay_ms(100).await;
    }
}
