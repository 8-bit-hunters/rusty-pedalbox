use core::sync::atomic::AtomicI8;
use embassy_stm32::rcc::{
    mux, AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv,
    PllSource, Sysclk,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::Config;
use embassy_usb::class::hid;
use static_cell::StaticCell;

#[repr(C, packed)]
pub struct PedalboxReport {
    pub x: i8,
    pub y: i8,
    pub z: i8,
    pub buttons: u8,
}

pub static AXIS_X: AtomicI8 = AtomicI8::new(0);
pub static AXIS_Y: AtomicI8 = AtomicI8::new(0);
pub static AXIS_Z: AtomicI8 = AtomicI8::new(0);

const PEDALBOX_REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, /*  Usage Page (Desktop),       */
    0x09, 0x04, /*  Usage (Joystick),           */
    0xA1, 0x01, /*  Collection (Application),   */
    0x05, 0x01, /*      Usage Page (Desktop),   */
    0x09, 0x30, /*      Usage (X),              */
    0x09, 0x31, /*      Usage (Y),              */
    0x09, 0x32, /*      Usage (Z),              */
    0x15, 0x81, /*      Logical Minimum (-127), */
    0x25, 0x7F, /*      Logical Maximum (127),  */
    0x75, 0x08, /*      Report Size (8),        */
    0x95, 0x03, /*      Report Count (3),       */
    0x81, 0x02, /*      Input (Variable),       */
    0x05, 0x09, /*      Usage Page (Button),    */
    0x19, 0x01, /*      Usage Minimum (01h),    */
    0x29, 0x01, /*      Usage Maximum (01h),    */
    0x14, /*      Logical Minimum (0),    */
    0x25, 0x01, /*      Logical Maximum (1),    */
    0x75, 0x01, /*      Report Size (1),        */
    0x95, 0x01, /*      Report Count (1),       */
    0x81, 0x02, /*      Input (Variable),       */
    0x75, 0x07, /*      Report Size (7),        */
    0x95, 0x01, /*      Report Count (1),       */
    0x81, 0x01, /*      Input (Constant),       */
    0xC0, /*  End Collection              */
];

pub static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
pub static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
pub static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
pub static MSOS_DESC: StaticCell<[u8; 128]> = StaticCell::new();
pub static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
pub static HID_STATE: StaticCell<hid::State<'static>> = StaticCell::new();

pub trait UsbConfiguration {
    fn usb_configuration() -> Config;
}

impl UsbConfiguration for Config {
    fn usb_configuration() -> Config {
        let mut config = Config::default();
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
}

pub trait PedalboxConfiguration {
    fn pedalbox_configuration() -> Self;
}

impl PedalboxConfiguration for hid::Config<'_> {
    fn pedalbox_configuration() -> Self {
        Self {
            report_descriptor: PEDALBOX_REPORT_DESCRIPTOR,
            request_handler: None,
            poll_ms: 10,
            max_packet_size: 8,
        }
    }
}

impl PedalboxConfiguration for embassy_usb::Config<'_> {
    fn pedalbox_configuration() -> Self {
        let mut config = embassy_usb::Config::new(0xcafe, 0x2025);
        config.manufacturer = Some("8 BitHunters");
        config.product = Some("Rusty Pedalbox");
        config.serial_number = Some("0001");
        config
    }
}
