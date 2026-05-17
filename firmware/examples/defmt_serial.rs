#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::peripherals::USB_OTG_FS;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource,
    Sysclk, mux,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::Timer;
use static_cell::StaticCell;
use panic_probe as _;

bind_interrupts!( struct Irqs {
    OTG_FS => InterruptHandler<peripherals::USB_OTG_FS>;
});

pub static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(usb_configuration());

    info!("USB CDC starting...");
    let ep_out_buffer = EP_OUT_BUFFER.init([0u8; 256]);

    let mut usb_config = embassy_stm32::usb::Config::default();
    usb_config.vbus_detection = false;

    // USB driver (PA11 = DM, PA12 = DP)
    let driver = Driver::new_fs(
        p.USB_OTG_FS,
        Irqs,
        p.PA12,
        p.PA11,
        ep_out_buffer,
        usb_config,
    );

    // USB config (this is your virtual COM identity)
    let mut config = embassy_usb::Config::new(0xCafe, 0x4001);
    config.manufacturer = Some("Rusty");
    config.product = Some("Pedalbox");
    config.max_packet_size_0 = 64;
    config.composite_with_iads = true;
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;

    spawner.spawn(usb_serial_task(driver, config).unwrap());
    spawner.spawn(hello_task().unwrap());
}

#[embassy_executor::task]
async fn usb_serial_task(
    driver: Driver<'static, USB_OTG_FS>,
    config: embassy_usb::Config<'static>,
) {
    defmt_embassy_usbserial::run(driver, config).await;
}

#[embassy_executor::task]
async fn hello_task() {
    loop {
        info!("USB connected");

        loop {
            info!("tick");
            Timer::after_millis(100).await;
        }
    }
}

fn usb_configuration() -> embassy_stm32::Config {
    let mut config = embassy_stm32::Config::default();
    config.rcc.sys = Sysclk::PLL1_P; // PLL1 P is the system clock
    config.rcc.mux.clk48sel = mux::Clk48sel::PLL1_Q; // PLL1 Q is the USB clock
    config.rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Bypass,
    });
    config.rcc.pll_src = PllSource::HSE;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV4,
        mul: PllMul::MUL168,
        divp: Some(PllPDiv::DIV2),
        divq: Some(PllQDiv::DIV7),
        divr: None,
    });
    config.rcc.ahb_pre = AHBPrescaler::DIV1; // 168 MHz
    config.rcc.apb1_pre = APBPrescaler::DIV4; // 42 MHz
    config.rcc.apb2_pre = APBPrescaler::DIV2; // 84 MHz
    config
}
