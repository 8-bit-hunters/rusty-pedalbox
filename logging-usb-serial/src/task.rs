//! On-device USB transport task.
//!
//! Feature-gated behind `usb` because it depends on `embassy-usb`, which does not build for
//! the host. It drives the host-tested [`flush_defmt`] and [`send_sensor_frame`] over a real
//! CDC-ACM `Sender`, which it adapts to the [`PacketSink`] trait.

use crate::controller::CONTROLLER;
use crate::transport::{PacketSink, SENSOR_CHANNEL, flush_defmt, send_sensor_frame};
use embassy_futures::join::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, Sender, State};
use embassy_usb::driver::{Driver, EndpointError};
use embassy_usb::{Builder, Config};
use static_cell::{ConstStaticCell, StaticCell};

impl<'d, D: Driver<'d>> PacketSink for Sender<'d, D> {
    type Error = EndpointError;

    async fn write_packet(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        Sender::write_packet(self, data).await
    }

    fn max_packet_size(&self) -> usize {
        Sender::max_packet_size(self) as usize
    }
}

static CONFIG_DESCRIPTOR_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0u8; 256]);
static BOS_DESCRIPTOR_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0u8; 256]);
static MSOS_DESCRIPTOR_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0u8; 256]);
static CONTROL_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0u8; 256]);
static STATE: StaticCell<State> = StaticCell::new();

/// Run the USB device and the logger flush loop together.
///
/// Builds a CDC-ACM device from `driver`/`config` and awaits both the USB stack and the
/// [`logger`] that drains the controller and sensor queue. See the crate docs for the
/// USB-CDC configuration requirements.
pub async fn run<D: Driver<'static>>(driver: D, config: Config<'static>) {
    let packet_size = config.max_packet_size_0 as u16;
    let state = STATE.init(State::new());

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESCRIPTOR_BUF.take(),
        BOS_DESCRIPTOR_BUF.take(),
        MSOS_DESCRIPTOR_BUF.take(),
        CONTROL_BUF.take(),
    );

    let class = CdcAcmClass::new(&mut builder, state, packet_size);
    let mut usb = builder.build();
    let (sender, _) = class.split();

    join(usb.run(), logger(sender)).await;
}

/// Flush loop: on each 20 ms tick send any buffered defmt data as a `0x01` frame, and send
/// each queued sensor sample as a `0x02` frame the moment it arrives. On a USB error the
/// controller is disabled and the loop waits for the next connection.
pub async fn logger<'d, D: Driver<'d>>(mut sender: Sender<'d, D>) {
    'main: loop {
        sender.wait_connection().await;
        CONTROLLER.enable();

        loop {
            match select(
                Timer::after(Duration::from_millis(20)),
                SENSOR_CHANNEL.receive(),
            )
            .await
            {
                Either::First(_) => {
                    if flush_defmt(&CONTROLLER, &mut sender).await.is_err() {
                        CONTROLLER.disable();
                        continue 'main;
                    }
                }
                Either::Second(sample) => {
                    if send_sensor_frame(&sample, &mut sender).await.is_err() {
                        CONTROLLER.disable();
                        continue 'main;
                    }
                }
            }
        }
    }
}
