use crate::buffer::LogBuffer;
use core::cell::UnsafeCell;
use core::sync::atomic::Ordering;
use portable_atomic::{AtomicBool, AtomicUsize};

pub(crate) const CONTROLLER: Controller = Controller::new();

pub(crate) struct Controller {
    active_buffer: AtomicUsize,
    enabled: AtomicBool, // The atomic handles the state mutation without needing exclusive access.
    buffers: [UnsafeCell<LogBuffer>; 2], // `RefCell` panics on conflicting borrows, `UnsafeCell` will have Undefined Behavior
}

impl Controller {
    pub(crate) const fn new() -> Self {
        Self {
            active_buffer: AtomicUsize::new(0),
            enabled: AtomicBool::new(true),
            buffers: [
                UnsafeCell::new(LogBuffer::new()),
                UnsafeCell::new(LogBuffer::new()),
            ],
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub(crate) fn has_flushing_buffer(&self) -> bool {
        (0..self.buffers.len()).any(|index| self.get_buffer(index).is_flushing())
    }

    pub(crate) fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        critical_section::with(|_| {
            (0..self.buffers.len())
                .map(|index| self.get_buffer_mut(index))
                .for_each(|buffer| buffer.reset());
        });
    }

    pub(crate) fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    pub(crate) fn write(&self, bytes: &[u8]) {
        if !self.is_enabled() {
            return;
        }

        let buffer = self.get_active_buffer_mut();

        if buffer.accepts(bytes.len()) {
            buffer.write(bytes);
        } else {
            self.swap_buffers();
            let buffer = self.get_active_buffer_mut();
            buffer.write(bytes);
        }
    }

    pub(crate) fn swap_buffers(&self) {
        if !self.is_enabled() {
            return;
        }

        let buffer = self.get_active_buffer_mut();
        buffer.mark_flush();
        self.active_buffer.store(
            self.active_buffer.load(Ordering::Relaxed) ^ 1,
            Ordering::Relaxed,
        );
    }

    pub(crate) async fn flush<F, E>(&self, mut flusher: F) -> Result<(), E>
    where
        F: AsyncFnMut(&[u8]) -> Result<(), E>,
    {
        let flushing = (0..self.buffers.len())
            .map(|idx| (idx, self.get_buffer(idx)))
            .find(|(_, buffer)| buffer.is_flushing());

        if let Some((idx, buffer)) = flushing {
            let result = flusher(buffer.filled()).await;
            self.reset_buffer(idx);
            result?
        }

        Ok(())
    }

    fn reset_buffer(&self, index: usize) {
        critical_section::with(|_| {
            self.get_buffer_mut(index).reset();
        });
    }

    fn get_active_buffer_mut(&self) -> &mut LogBuffer {
        let active = self.active_buffer.load(Ordering::Relaxed);
        self.get_buffer_mut(active)
    }

    fn get_buffer(&self, index: usize) -> &LogBuffer {
        unsafe { &*self.buffers[index].get() }
    }

    fn get_buffer_mut(&self, index: usize) -> &mut LogBuffer {
        unsafe { &mut *self.buffers[index].get() }
    }
}

unsafe impl Sync for Controller {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::BUFFER_SIZE;
    use core::cell::Cell;

    mod test_creation {
        use super::*;

        #[test]
        fn when_new_controller_is_created() {
            // When
            let ctrl = Controller::new();

            // Then
            assert!(ctrl.is_enabled(), "Controller should be enabled");
            assert_eq!(
                ctrl.has_flushing_buffer(),
                false,
                "Controller should not have a flushing buffer"
            );
        }
    }

    mod test_enable_disable {
        use super::*;

        #[test]
        fn when_controller_is_disabled() {
            // Given
            let ctrl = Controller::new().and_has_a_flushing_buffer_with(&[1, 2, 3]);

            // When
            ctrl.disable();

            // Then
            assert_eq!(ctrl.is_enabled(), false, "Controller should be disabled");
            assert_eq!(
                ctrl.has_flushing_buffer(),
                false,
                "Controller should not have flushing buffers"
            );
        }

        #[test]
        fn when_disabled_controller_is_enabled() {
            // Given
            let ctrl = Controller::new().and_disabled();

            // When
            ctrl.enable();

            // Then
            assert!(ctrl.is_enabled(), "Controller should be enabled");
        }
    }

    mod test_write {
        use super::*;

        #[test]
        fn when_controller_is_enabled() {
            // Given
            let ctrl = Controller::new().and_enabled();
            let data = [1, 2, 3, 4];

            // When
            ctrl.write(&data);

            // Then
            assert_eq!(
                get_active_buffer_from(&ctrl).filled(),
                data,
                "Buffer should have the data"
            );
        }

        #[test]
        fn when_controller_is_disabled() {
            // Given
            let ctrl = Controller::new().and_disabled();
            let data = [1, 2, 3, 4];

            // When
            ctrl.write(&data);

            // Then
            assert_eq!(
                get_active_buffer_from(&ctrl).filled(),
                [],
                "Buffer should be empty"
            );
        }

        #[test]
        fn when_active_buffer_is_full() {
            // Given
            let full_buffer = 0;
            let ctrl = Controller::new()
                .and_enabled()
                .and_active_buffer_index_is(full_buffer)
                .and_active_buffer_is_full();

            let data = [1, 2, 3];

            // When
            ctrl.write(&data);

            // Then
            assert_eq!(
                get_buffer_from(&ctrl, full_buffer).filled(),
                &full(),
                "Buffer should be full"
            );
            assert!(
                get_buffer_from(&ctrl, full_buffer).is_flushing(),
                "Full buffer should be flushing"
            );
            assert_eq!(
                get_active_buffer_from(&ctrl).filled(),
                &data,
                "Fresh buffer should have the data"
            );
        }
    }

    mod test_swap {
        use super::*;

        #[test]
        fn when_controller_is_enabled() {
            // Given
            let original = 0;
            let ctrl = Controller::new()
                .and_enabled()
                .and_does_not_have_flushing_buffer()
                .and_active_buffer_index_is(original);

            // When
            ctrl.swap_buffers();

            // Then
            assert_eq!(
                ctrl.has_flushing_buffer(),
                true,
                "Controller should have a flushing buffer"
            );
            assert_eq!(
                get_buffer_from(&ctrl, original).is_flushing(),
                true,
                "Original buffer should be flushing"
            );
            assert_eq!(
                get_buffer_from(&ctrl, 1),
                get_active_buffer_from(&ctrl),
                "Buffers should be swapped"
            );
        }

        #[test]
        fn when_controller_is_disabled() {
            // Given
            let original = 0;
            let ctrl = Controller::new()
                .and_disabled()
                .and_does_not_have_flushing_buffer()
                .and_active_buffer_index_is(original);

            // When
            ctrl.swap_buffers();

            // Then
            assert_eq!(
                ctrl.has_flushing_buffer(),
                false,
                "Controller should not have flushing buffer"
            );
            assert_eq!(
                get_buffer_from(&ctrl, original).is_flushing(),
                false,
                "Original buffer should not be flushing"
            );
            assert_eq!(
                get_buffer_from(&ctrl, original),
                get_active_buffer_from(&ctrl),
                "Buffers should not be swapped"
            );
        }
    }

    mod test_flush {
        use super::*;

        #[test]
        fn when_controller_has_no_flushing_buffer() {
            // Given
            let ctrl = Controller::new()
                .and_enabled()
                .and_does_not_have_flushing_buffer();
            let mut called = false;

            // When
            let result: Result<(), ()> = pollster::block_on(ctrl.flush(async |_| {
                called = true;
                Ok(())
            }));

            // Then
            assert_eq!(called, false, "Closure should not be called");
            assert!(result.is_ok(), "Flush should be ok");
        }

        #[test]
        fn when_controller_has_flushing_buffer() {
            // Given
            let data = [1, 2, 3, 4];
            let ctrl = Controller::new()
                .and_enabled()
                .and_has_a_flushing_buffer_with(&data);

            let called = core::cell::Cell::new(false);
            let capture = CapturedData::new();

            // When
            let result: Result<(), ()> = pollster::block_on(ctrl.flush(async |bytes| {
                called.set(true);
                capture.set(bytes);
                Ok(())
            }));

            // Then
            assert!(called.get(), "Closure should be called");
            assert!(result.is_ok(), "Flush should be ok");
            assert_eq!(capture, data, "Data should be flushed by the flusher");
        }

        #[test]
        fn when_flush_succeeds() {
            // Given
            let ctrl = Controller::new()
                .and_enabled()
                .and_has_a_flushing_buffer_with(&[1, 2, 3, 4]);

            // When
            let _: Result<(), ()> = pollster::block_on(ctrl.flush(async |_| Ok(())));

            // Then
            assert_eq!(
                ctrl.has_flushing_buffer(),
                false,
                "Controller should not have a flushing buffer"
            );
        }

        #[test]
        fn when_flush_fails() {
            // Given
            let ctrl = Controller::new()
                .and_enabled()
                .and_has_a_flushing_buffer_with(&[1, 2, 3, 4]);

            // When
            let result: Result<(), &str> =
                pollster::block_on(ctrl.flush(async |_| Err("some error")));

            // Then
            assert!(result.is_err(), "Flush should be error");
            assert_eq!(
                result,
                Err("some error"),
                "The flush error should be propagated"
            );
            assert_eq!(
                ctrl.has_flushing_buffer(),
                false,
                "Controller should not have flushing buffer"
            );
        }

        #[test]
        fn when_both_buffers_are_flushing() {
            // Given
            let data_a = [1u8, 2, 3, 4];
            let data_b = [5u8, 6, 7, 8];
            let ctrl = Controller::new()
                .and_enabled()
                .and_has_a_flushing_buffer_with(&data_a)
                .and_has_a_flushing_buffer_with(&data_b);

            let capture = CapturedData::new();

            // When
            let _: Result<(), ()> = pollster::block_on(ctrl.flush(async |bytes| {
                capture.set(bytes);
                Ok(())
            }));

            // Then
            assert_eq!(capture, data_a, "Buffer 0 should be flushed first");
        }
    }

    fn get_active_buffer_from(controller: &Controller) -> &LogBuffer {
        let index = controller.active_buffer.load(Ordering::Relaxed);
        controller.get_buffer(index)
    }

    fn get_buffer_from(controller: &Controller, index: usize) -> &LogBuffer {
        controller.get_buffer(index)
    }

    const fn full() -> [u8; BUFFER_SIZE] {
        [0u8; BUFFER_SIZE]
    }

    trait Preconditions {
        fn and_enabled(self) -> Self;
        fn and_disabled(self) -> Self;
        fn and_has_a_flushing_buffer_with(self, data: &[u8]) -> Self;
        fn and_does_not_have_flushing_buffer(self) -> Self;
        fn and_active_buffer_index_is(self, index: usize) -> Self;
        fn and_active_buffer_is_full(self) -> Self;
    }

    impl Preconditions for Controller {
        fn and_enabled(self) -> Self {
            self.enable();
            assert!(
                self.is_enabled(),
                "Precondition failed: controller is not enabled"
            );
            self
        }

        fn and_disabled(self) -> Self {
            self.disable();
            assert_eq!(
                self.is_enabled(),
                false,
                "Precondition failed: controller is not disabled"
            );
            self
        }

        fn and_has_a_flushing_buffer_with(self, data: &[u8]) -> Self {
            self.write(data);
            self.swap_buffers();
            self
        }

        fn and_does_not_have_flushing_buffer(self) -> Self {
            assert_eq!(
                self.has_flushing_buffer(),
                false,
                "Precondition failed: controller has a flushing buffer"
            );
            self
        }

        fn and_active_buffer_index_is(self, index: usize) -> Self {
            let active_buffer = self.active_buffer.load(Ordering::Relaxed);
            assert_eq!(
                index, active_buffer,
                "Precondition failed: Buffer {} is not the active buffer",
                index
            );
            self
        }

        fn and_active_buffer_is_full(self) -> Self {
            self.write(&full());
            let active_buffer = self.get_active_buffer_mut();
            assert_eq!(
                active_buffer.is_flushing(),
                true,
                "Precondition failed: The active buffer is not full"
            );
            self
        }
    }

    #[derive(Debug)]
    struct CapturedData {
        data: Cell<[u8; BUFFER_SIZE]>,
        len: Cell<usize>,
    }

    impl CapturedData {
        fn new() -> Self {
            Self {
                data: Cell::new([0u8; BUFFER_SIZE]),
                len: Cell::new(0),
            }
        }

        fn set(&self, bytes: &[u8]) {
            let mut buf = [0u8; BUFFER_SIZE];
            buf[..bytes.len()].copy_from_slice(bytes);
            self.data.set(buf);
            self.len.set(bytes.len());
        }
    }

    impl<const N: usize> PartialEq<[u8; N]> for CapturedData {
        fn eq(&self, other: &[u8; N]) -> bool {
            let data = self.data.get();
            &data[..self.len.get()] == other.as_slice()
        }
    }
}
