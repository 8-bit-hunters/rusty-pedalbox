use crate::storage::Storage;
use bytemuck::{bytes_of, Pod, Zeroable};
use embassy_stm32::flash::{Blocking, Flash};

const FLASH_STORAGE_ADDR: u32 = 128 * 1024;
const FLASH_STORAGE_SIZE: u32 = 128 * 1024;

pub struct FlashStorage<const N: usize> {
    flash: Flash<'static, Blocking>,
    address: u32,
    page_size: u32,
}

impl<const N: usize> FlashStorage<N> {
    pub fn new(flash: Flash<'static, Blocking>) -> FlashStorage<N> {
        Self {
            flash,
            address: FLASH_STORAGE_ADDR,
            page_size: FLASH_STORAGE_SIZE,
        }
    }
}

impl<T, const N: usize> Storage<T> for FlashStorage<N>
where
    T: Clone + Copy + Pod + Zeroable,
{
    type Error = embassy_stm32::flash::Error;

    fn load(&mut self) -> Option<T> {
        let mut buf = [0u8; N];

        self.flash.blocking_read(self.address, &mut buf).ok()?;
        let data = bytemuck::from_bytes::<T>(&buf);

        Some(*data)
    }

    fn save(&mut self, value: &T) -> Result<(), Self::Error> {
        let data = *value;
        let bytes = bytes_of(&data);

        self.flash
            .blocking_erase(self.address, self.address + self.page_size)?;

        self.flash.blocking_write(self.address, bytes)?;

        Ok(())
    }
}
