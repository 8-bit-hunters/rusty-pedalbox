#![no_std]
#![no_main]

mod board;
mod usb;

use core::sync::atomic::Ordering;
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use crate::board::Board;
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
use rusty_pedalbox::fmt::warn;
use rusty_pedalbox::prelude::{
    AnalogMonitor, AnalogMonitorConfig, LoadCellMonitor, LoadCellMonitorConfig,
};

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

    let load_cell = Hx711::new(Delay, board.brake_data, board.brake_clock)
        .expect("Failed to create HX711 driver");

    let hid_writer = HidWriter::<_, 8>::new(
        &mut builder,
        hid_state,
        hid::Config::pedalbox_configuration(),
    );

    spawner
        .spawn(hid_task(hid_writer))
        .expect("Failed to spawn hid task");

    let gas_pedal = AnalogMonitor::new(
        "GAS_PEDAL",
        AnalogMonitorConfig {
            range_min: 1820,
            range_max: 3100,
            adc: Adc::new(board.gas_adc),
            pin: board.gas_potentiometer,
            output_channel: &AXIS_X,
        },
    );

    let brake_pedal = LoadCellMonitor::new(
        "BRAKE_PEDAL",
        LoadCellMonitorConfig {
            range_min: 0,
            range_max: 230_000,
            load_cell,
            output_channel: &AXIS_Y,
        },
    );

    let clutch_pedal = AnalogMonitor::new(
        "CLUTCH_PEDAL",
        AnalogMonitorConfig {
            range_min: u16::MIN,
            range_max: u16::MAX,
            adc: Adc::new(board.clutch_adc),
            pin: board.clutch_potentiometer,
            output_channel: &AXIS_Z,
        },
    );

    spawner
        .spawn(input_monitor_x(gas_pedal))
        .expect("Failed to spawn input monitor X");
    spawner
        .spawn(input_monitor_z(clutch_pedal))
        .expect("Failed to spawn input monitor Z");
    spawner
        .spawn(input_monitor_y(brake_pedal))
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
async fn input_monitor_x(mut monitor: AnalogMonitor<Adc<'static, ADC1>, Peri<'static, PA7>, u16>) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(5)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_z(mut monitor: AnalogMonitor<Adc<'static, ADC2>, Peri<'static, PA5>, u16>) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(5)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_y(
    mut monitor: LoadCellMonitor<Hx711<Delay, Input<'static>, Output<'static>>, i32>,
) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(10)).await;
    }
}
