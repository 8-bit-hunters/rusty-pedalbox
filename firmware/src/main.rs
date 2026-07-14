#![no_std]
#![no_main]

mod board;
#[cfg(feature = "log-usb")]
mod telemetry;
mod usb;

use core::sync::atomic::Ordering;
use panic_probe as _;

#[cfg(feature = "log-rtt")]
use defmt_rtt as _;

#[cfg(all(feature = "log-rtt", feature = "log-usb"))]
compile_error!("enable only one logging backend: `log-rtt` or `log-usb`");
#[cfg(not(any(feature = "log-rtt", feature = "log-usb")))]
compile_error!("enable a logging backend: `log-rtt` or `log-usb`");

use crate::board::Board;
use crate::usb::{
    AXIS_X, AXIS_Y, AXIS_Z, BOS_DESC, CONFIG_DESC, CONTROL_BUF, EP_OUT_BUFFER, HID_STATE,
    MSOS_DESC, PedalboxConfiguration, PedalboxReport, UsbConfiguration,
};
use embassy_executor::Spawner;
use embassy_stm32::adc::Adc;
use embassy_stm32::gpio::{Input, Output};
use embassy_stm32::peripherals::{ADC1, ADC2, PA5, PA7, USB_OTG_FS};
use embassy_stm32::{Config, Peri};
use embassy_time::{Delay, Duration, Timer};
use embassy_usb::Builder;
use embassy_usb::class::hid;
use embassy_usb::class::hid::HidWriter;
use hx711::Hx711;
use pedalbox_lib::calibration::fixed::FixedRange;
use pedalbox_lib::fmt::warn;
use pedalbox_lib::io_monitors::{
    AnalogMonitor, AnalogMonitorConfig, LoadCellMonitor, LoadCellMonitorConfig,
};
#[cfg(feature = "log-usb")]
use telemetry::{BRAKE_RAW, CLUTCH_RAW, GAS_RAW, telemetry_task};

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

    let hid_writer = HidWriter::<_, 8>::new(
        &mut builder,
        hid_state,
        hid::Config::pedalbox_configuration(),
    );
    spawner.spawn(hid_task(hid_writer).expect("Failed to create hid task token"));

    // Add the CDC-ACM logging interface to the same (composite) builder.
    #[cfg(feature = "log-usb")]
    let cdc_class = {
        use crate::usb::CDC_STATE;
        let cdc_state = CDC_STATE.init(embassy_usb::class::cdc_acm::State::new());
        embassy_usb::class::cdc_acm::CdcAcmClass::new(&mut builder, cdc_state, 64)
    };

    let usb = builder.build();
    spawner.spawn(usb_task(usb).expect("Failed to create usb task token"));

    #[cfg(feature = "log-usb")]
    {
        let (cdc_sender, _cdc_receiver) = cdc_class.split();
        spawner.spawn(logger_task(cdc_sender).expect("Failed to create logger task token"));
    }

    let gas_pedal_range = FixedRange::default().min(2644).max(3700);
    let gas_pedal = AnalogMonitor::new(
        "GAS_PEDAL",
        AnalogMonitorConfig {
            range: gas_pedal_range,
            adc: Adc::new(board.gas_adc),
            pin: board.gas_potentiometer,
            output_channel: &AXIS_X,
            #[cfg(feature = "log-usb")]
            raw_channel: Some(&GAS_RAW),
            #[cfg(feature = "log-rtt")]
            raw_channel: None,
        },
    );
    spawner.spawn(input_monitor_x(gas_pedal).expect("Failed to create input monitor X task token"));

    let brake_pedal_range = FixedRange::default().min(0).max(230_000);
    let brake_pedal = LoadCellMonitor::new(
        "BRAKE_PEDAL",
        LoadCellMonitorConfig {
            range: brake_pedal_range,
            load_cell: Hx711::new(Delay, board.brake_data, board.brake_clock)
                .expect("Failed to create HX711 driver"),
            output_channel: &AXIS_Y,
            #[cfg(feature = "log-usb")]
            raw_channel: Some(&BRAKE_RAW),
            #[cfg(feature = "log-rtt")]
            raw_channel: None,
        },
    );
    spawner
        .spawn(input_monitor_y(brake_pedal).expect("Failed to create input monitor Y task token"));

    let clutch_pedal_range = FixedRange::default().min(265).max(1700);
    let clutch_pedal = AnalogMonitor::new(
        "CLUTCH_PEDAL",
        AnalogMonitorConfig {
            range: clutch_pedal_range,
            adc: Adc::new(board.clutch_adc),
            pin: board.clutch_potentiometer,
            output_channel: &AXIS_Z,
            #[cfg(feature = "log-usb")]
            raw_channel: Some(&CLUTCH_RAW),
            #[cfg(feature = "log-rtt")]
            raw_channel: None,
        },
    );
    spawner
        .spawn(input_monitor_z(clutch_pedal).expect("Failed to create input monitor Z task token"));

    #[cfg(feature = "log-usb")]
    spawner.spawn(telemetry_task().expect("Failed to create telemetry task token"));
}

#[embassy_executor::task]
async fn usb_task(
    mut device: embassy_usb::UsbDevice<'static, embassy_stm32::usb::Driver<'static, USB_OTG_FS>>,
) {
    device.run().await;
}

#[cfg(feature = "log-usb")]
#[embassy_executor::task]
async fn logger_task(
    sender: embassy_usb::class::cdc_acm::Sender<
        'static,
        embassy_stm32::usb::Driver<'static, USB_OTG_FS>,
    >,
) {
    logging_usb_serial::logger(sender).await;
}

#[embassy_executor::task]
async fn hid_task(
    mut writer: HidWriter<'static, embassy_stm32::usb::Driver<'static, USB_OTG_FS>, 8>,
) {
    loop {
        let report = PedalboxReport {
            x: AXIS_X.load(Ordering::Relaxed),
            y: AXIS_Y.load(Ordering::Relaxed),
            z: AXIS_Z.load(Ordering::Relaxed),
            buttons: 0,
        };

        let bytes = bytemuck::bytes_of(&report);
        if let Err(_e) = writer.write(bytes).await {
            warn!("HID write failed");
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_x(
    mut monitor: AnalogMonitor<Adc<'static, ADC1>, Peri<'static, PA7>, FixedRange<u16>, u16>,
) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(5)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_z(
    mut monitor: AnalogMonitor<Adc<'static, ADC2>, Peri<'static, PA5>, FixedRange<u16>, u16>,
) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(5)).await;
    }
}

#[embassy_executor::task]
async fn input_monitor_y(
    mut monitor: LoadCellMonitor<
        Hx711<Delay, Input<'static>, Output<'static>>,
        FixedRange<i32>,
        i32,
    >,
) {
    loop {
        monitor.run();
        Timer::after(Duration::from_millis(10)).await;
    }
}
