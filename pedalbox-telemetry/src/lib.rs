#![no_std]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Value {
    U16(u16),
    U32(u32),
    I16(i16),
    I32(i32),
    F32(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SensorSample {
    pub timestamp_ms: u32,
    pub channel_id: u8,
    pub value: Value,
}
