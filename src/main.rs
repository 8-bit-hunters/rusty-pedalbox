#![no_std]
#![no_main]

mod board;
mod fmt;
mod usb;

use core::sync::atomic::Ordering;
use defmt::warn;
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use crate::board::Board;
use crate::fmt::info;
use crate::usb::{
    PedalboxConfiguration, PedalboxReport, UsbConfiguration, AXIS_X, AXIS_Y, AXIS_Z, BOS_DESC,
    CONFIG_DESC, CONTROL_BUF, EP_OUT_BUFFER, HID_STATE, MSOS_DESC,
};
use embassy_executor::Spawner;
use embassy_stm32::adc::Adc;
use embassy_stm32::gpio::{Input, Output};
use embassy_stm32::peripherals::{ADC1, ADC2, PA5, PA7, USB_OTG_FS};
use embassy_stm32::{Config, Peri};
use embassy_time::{Delay, Duration, Timer};
use embassy_usb::class::hid;
use embassy_usb::class::hid::HidWriter;
use embassy_usb::Builder;
use hx711::Hx711;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Config::usb_configuration());
    let board = Board::new(p);

    let ep_out_buffer = EP_OUT_BUFFER.init([0; 256]);
    let config_desc = CONFIG_DESC.init([0; 256]);
    let bos_desc = BOS_DESC.init([0; 256]);
    let msos_desc = MSOS_DESC.init([0; 128]);
    let control_buf = CONTROL_BUF.init([0; 64]);
    let hid_state = HID_STATE.init(hid::State::new());

    let driver = embassy_stm32::usb::Driver::new_fs(
        board.usb_peripheral,
        board.usb_interrupt,
        board.usb_d_plus,
        board.usb_d_minus,
        ep_out_buffer,
        embassy_stm32::usb::Config::default(),
    );

    let mut builder = Builder::new(
        driver,
        embassy_usb::Config::pedalbox_configuration(),
        config_desc,
        bos_desc,
        msos_desc,
        control_buf,
    );

    let adc_x = Adc::new(board.gas_adc);
    let adc_z = Adc::new(board.clutch_adc);
    let load_cell_y = Hx711::new(Delay, board.brake_data, board.brake_clock)
        .expect("Failed to create HX711 driver");

    let hid_writer = HidWriter::<_, 8>::new(
        &mut builder,
        hid_state,
        hid::Config::pedalbox_configuration(),
    );

    spawner
        .spawn(hid_task(hid_writer))
        .expect("Failed to spawn hid task");

    spawner
        .spawn(input_monitor_x(adc_x, board.gas_potentiometer))
        .expect("Failed to spawn input monitor X");
    spawner
        .spawn(input_monitor_z(adc_z, board.clutch_potentiometer))
        .expect("Failed to spawn input monitor Z");
    spawner
        .spawn(input_monitor_y(load_cell_y))
        .expect("Failed to spawn input monitor Y");

    let usb = builder.build();
    spawner
        .spawn(usb_task(usb))
        .expect("Failed to spawn usb task")
}

#[embassy_executor::task]
async fn usb_task(
    mut device: embassy_usb::UsbDevice<'static, embassy_stm32::usb::Driver<'static, USB_OTG_FS>>,
) {
    device.run().await;
}

#[embassy_executor::task]
async fn hid_task(
    mut writer: HidWriter<'static, embassy_stm32::usb::Driver<'static, USB_OTG_FS>, 8>,
) {
    loop {
        let x = AXIS_X.load(Ordering::Relaxed);
        let y = AXIS_Y.load(Ordering::Relaxed);
        let z = AXIS_Z.load(Ordering::Relaxed);

        let report = PedalboxReport {
            x,
            y,
            z,
            buttons: 0,
        };

        let bytes = unsafe {
            core::slice::from_raw_parts(
                &report as *const PedalboxReport as *const u8,
                core::mem::size_of::<PedalboxReport>(),
            )
        };

        if let Err(e) = writer.write(bytes).await {
            warn!("HID write failed: {:?}", e);
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_x(mut adc: Adc<'static, ADC1>, mut pin: Peri<'static, PA7>) {
    loop {
        let adc_value = adc.blocking_read(&mut pin);
        let scaled_value = ((adc_value as i32 * (i16::MAX as i32 - i16::MIN as i32) / 3200)
            + i16::MIN as i32) as i16;
        info!("Potentiometer X:\t{}\t{}", adc_value, scaled_value);
        AXIS_X.store(scaled_value, Ordering::Relaxed);
        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_z(mut adc: Adc<'static, ADC2>, mut pin: Peri<'static, PA5>) {
    loop {
        let adc_value = adc.blocking_read(&mut pin);
        let scaled_value = ((adc_value as i32 * (i16::MAX as i32 - i16::MIN as i32) / 3200)
            + i16::MIN as i32) as i16;
        info!("Potentiometer Z:\t{}\t{}", adc_value, scaled_value);
        AXIS_Z.store(scaled_value, Ordering::Relaxed);
        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_y(mut load_cell: Hx711<Delay, Input<'static>, Output<'static>>) {
    loop {
        match load_cell.retrieve() {
            Ok(v) => {
                let scaled_value = ((v as i64 * i16::MAX as i64) / 250_000)
                    .clamp(i16::MIN as i64, i16::MAX as i64)
                    as i16;
                info!("Load cell Y:\t{}\t{}", v, scaled_value);
                AXIS_Y.store(scaled_value, Ordering::Relaxed);
            }
            Err(_) => {
                warn!("couldn't retrieve data")
            }
        }
        Timer::after(Duration::from_millis(10)).await;
    }
}
