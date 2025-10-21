use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::peripherals::{ADC1, ADC2, PA11, PA12, PA5, PA7, USB_OTG_FS};
use embassy_stm32::{bind_interrupts, usb, Peri, Peripherals};

bind_interrupts!(pub struct Irqs {
    OTG_FS => usb::InterruptHandler<USB_OTG_FS>;
});

pub struct Board {
    pub usb_peripheral: Peri<'static, USB_OTG_FS>,
    pub usb_interrupt: Irqs,
    pub usb_d_plus: Peri<'static, PA12>,
    pub usb_d_minus: Peri<'static, PA11>,
    pub gas_adc: Peri<'static, ADC1>,
    pub gas_potentiometer: Peri<'static, PA7>,
    pub clutch_adc: Peri<'static, ADC2>,
    pub clutch_potentiometer: Peri<'static, PA5>,
    pub brake_data: Input<'static>,
    pub brake_clock: Output<'static>,
}

impl Board {
    pub fn new(peripherals: Peripherals) -> Self {
        let brake_data = Input::new(peripherals.PC11, Pull::None);
        let brake_clock = Output::new(peripherals.PC12, Level::Low, Speed::High);
        Self {
            usb_peripheral: peripherals.USB_OTG_FS,
            usb_interrupt: Irqs,
            usb_d_plus: peripherals.PA12,
            usb_d_minus: peripherals.PA11,
            gas_adc: peripherals.ADC1,
            gas_potentiometer: peripherals.PA7,
            clutch_adc: peripherals.ADC2,
            clutch_potentiometer: peripherals.PA5,
            brake_data,
            brake_clock,
        }
    }
}
