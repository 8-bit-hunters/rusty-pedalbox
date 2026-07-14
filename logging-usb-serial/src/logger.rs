use crate::controller::CONTROLLER;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

static USB_ENCODER: UsbEncoder = UsbEncoder::new();

struct UsbEncoder {
    /// Boolean lock: `true` once `acquire` has taken exclusive access.
    taken: AtomicBool,
    /// Critical-section restore token, needed to exit the section in `release`.
    restore: UnsafeCell<critical_section::RestoreState>,
    /// defmt frame encoder.
    encoder: UnsafeCell<defmt::Encoder>,
}

impl UsbEncoder {
    const fn new() -> UsbEncoder {
        Self {
            taken: AtomicBool::new(false),
            restore: UnsafeCell::new(critical_section::RestoreState::invalid()),
            encoder: UnsafeCell::new(defmt::Encoder::new()),
        }
    }

    /// Enter a critical section and begin a defmt frame.
    ///
    /// # Panics
    /// Panics if acquired re-entrantly.
    fn acquire(&self) {
        // SAFETY: paired with `release`, per the Logger trait contract.
        let restore_state = unsafe { critical_section::acquire() };

        if self.taken.load(Ordering::Relaxed) {
            panic!("defmt logger taken reentrantly");
        }
        self.taken.store(true, Ordering::Relaxed);

        // SAFETY: we hold the critical section, so exclusive access to the cells is sound.
        unsafe {
            self.restore.get().write(restore_state);
            let encoder = &mut *self.encoder.get();
            encoder.start_frame(Self::inner)
        }
    }

    /// End the defmt frame and leave the critical section.
    ///
    /// # Safety
    /// Must be called exactly once after `acquire`.
    unsafe fn release(&self) {
        if !self.taken.load(Ordering::Relaxed) {
            panic!("defmt release outside of critical section.")
        }
        // SAFETY: still inside the critical section entered by `acquire`.
        unsafe {
            let encoder = &mut *self.encoder.get();
            encoder.end_frame(Self::inner);

            let restore_state = self.restore.get().read();
            self.taken.store(false, Ordering::Relaxed);
            critical_section::release(restore_state);
        }
    }

    /// Flush the current buffer by swapping the controller's double buffer.
    ///
    /// # Safety
    /// Must be called between `acquire` and `release`.
    unsafe fn flush(&self) {
        CONTROLLER.swap_buffers();
    }

    /// Write bytes into the defmt encoder.
    ///
    /// # Safety
    /// Must be called between `acquire` and `release`.
    unsafe fn write(&self, bytes: &[u8]) {
        let encoder = unsafe { &mut *self.encoder.get() };
        encoder.write(bytes, Self::inner);
    }

    /// Sink for encoded bytes: append them to the controller's active buffer.
    fn inner(bytes: &[u8]) {
        // SAFETY: only ever called from within the critical section held above.
        CONTROLLER.write(&bytes);
    }
}

unsafe impl Sync for UsbEncoder {}

#[defmt::global_logger]
struct UsbLogger;

unsafe impl defmt::Logger for UsbLogger {
    fn acquire() {
        USB_ENCODER.acquire();
    }

    unsafe fn flush() {
        unsafe { USB_ENCODER.flush() };
    }

    unsafe fn release() {
        unsafe { USB_ENCODER.release() };
    }

    unsafe fn write(bytes: &[u8]) {
        unsafe { USB_ENCODER.write(bytes) };
    }
}
