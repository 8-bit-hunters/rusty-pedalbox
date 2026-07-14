use crate::usb::{AXIS_X, AXIS_Y, AXIS_Z};
use core::sync::atomic::{AtomicI32, AtomicU16, Ordering};
use embassy_time::{Duration, Instant, Timer};
use logging_usb_serial::{SensorSample, Value, send_sensor};

pub static GAS_RAW: AtomicU16 = AtomicU16::new(0);
pub static BRAKE_RAW: AtomicI32 = AtomicI32::new(0);
pub static CLUTCH_RAW: AtomicU16 = AtomicU16::new(0);

enum ChannelIds {
    GasRaw = 0,
    GasCalibrated = 1,
    BrakeRaw = 2,
    BrakeCalibrated = 3,
    ClutchRaw = 4,
    ClutchCalibrated = 5,
}

#[embassy_executor::task]
pub async fn telemetry_task() {
    loop {
        Timer::after(Duration::from_millis(100)).await;
        let ts = Instant::now().as_millis() as u32;
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::GasRaw as u8,
            value: Value::U16(GAS_RAW.load(Ordering::Relaxed)),
        });
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::GasCalibrated as u8,
            value: Value::I16(AXIS_X.load(Ordering::Relaxed)),
        });
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::BrakeRaw as u8,
            value: Value::I32(BRAKE_RAW.load(Ordering::Relaxed)),
        });
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::BrakeCalibrated as u8,
            value: Value::I16(AXIS_Y.load(Ordering::Relaxed)),
        });
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::ClutchRaw as u8,
            value: Value::U16(CLUTCH_RAW.load(Ordering::Relaxed)),
        });
        send_sensor(SensorSample {
            timestamp_ms: ts,
            channel_id: ChannelIds::ClutchCalibrated as u8,
            value: Value::I16(AXIS_Z.load(Ordering::Relaxed)),
        });
    }
}
