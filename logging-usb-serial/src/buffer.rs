use core::cmp::min;

const BUFFER_SIZE: usize = 256;
const MAX_ALLOWED_SIZE: usize = BUFFER_SIZE - 2;

pub(crate) struct LogBuffer {
    cursor: usize,
    data: [u8; BUFFER_SIZE],
    state: BufferState,
}

impl LogBuffer {
    pub const fn new() -> Self {
        Self {
            cursor: 0,
            data: [0u8; 256],
            state: BufferState::Active,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == BufferState::Active
    }

    pub fn is_flushing(&self) -> bool {
        self.state == BufferState::Flush
    }

    pub fn is_almost_full(&self) -> bool {
        self.cursor >= MAX_ALLOWED_SIZE
    }

    pub fn len(&self) -> usize {
        self.cursor
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if self.state == BufferState::Flush {
            return;
        }

        let number_of_bytes_to_add = min(self.data.len() - self.cursor, bytes.len());
        self.data[self.cursor..self.cursor + number_of_bytes_to_add]
            .copy_from_slice(&bytes[..number_of_bytes_to_add]);
        self.cursor += number_of_bytes_to_add;

        if self.is_almost_full() {
            self.mark_flush()
        }
    }

    pub fn filled(&self) -> &[u8] {
        &self.data[..self.cursor]
    }

    pub fn reset(&mut self) {
        self.cursor = 0;
        self.state = BufferState::Active;
    }

    pub fn accepts(&self, n: usize) -> bool {
        self.is_active() && self.cursor + n <= BUFFER_SIZE
    }

    pub fn mark_flush(&mut self) {
        self.state = BufferState::Flush;
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum BufferState {
    Active,
    Flush,
}

#[cfg(test)]
mod tests {
    use super::LogBuffer;

    mod test_creation {
        use super::*;

        #[test]
        fn when_new_buffer_is_created() {
            // When
            let buf = LogBuffer::new();

            // Then
            assert_eq!(buf.len(), 0);
            assert!(buf.is_active());
        }
    }

    mod test_write {
        use super::*;
        use crate::buffer::BUFFER_SIZE;

        #[test]
        fn when_it_is_called_on_active_buffer() {
            // Given
            let mut buf = LogBuffer::new();
            let data = [1, 2, 3];

            // When
            buf.write(&data);

            // Then
            assert_eq!(buf.len(), 3);
            assert_eq!(buf.filled(), &data);
        }

        #[test]
        fn when_it_is_called_with_oversized_payload() {
            // Given
            let mut buf = LogBuffer::new();
            let oversized = [0xAAu8; BUFFER_SIZE + 10];

            // When
            buf.write(&oversized);

            // Then
            assert_eq!(buf.len(), BUFFER_SIZE);
        }

        #[test]
        fn when_it_is_called_on_a_flushing_buffer() {
            // Given
            let mut buf = LogBuffer::new();
            buf.mark_flush();

            // When
            buf.write(&[1, 2, 3]);

            // Then
            assert_eq!(buf.len(), 0);
        }

        #[test]
        fn when_it_is_called_on_a_nearly_full_buffer() {
            // Given
            let mut buf = LogBuffer::new();
            let nearly_full = [0u8; BUFFER_SIZE - 2];

            // When
            buf.write(&nearly_full);

            // Then
            assert!(buf.is_flushing())
        }
    }

    mod test_reset {
        use super::*;

        #[test]
        fn when_reset_is_called() {
            // Given
            let mut buf = LogBuffer::new();
            buf.write(&[1, 2, 3]);
            buf.mark_flush();

            // When
            buf.reset();

            // Then
            assert!(buf.is_active());
            assert_eq!(buf.len(), 0);
        }
    }

    mod test_accepts {
        use super::*;
        use crate::buffer::BUFFER_SIZE;

        #[test]
        fn when_buffer_is_active_and_has_space() {
            // Given
            let buf = LogBuffer::new();

            // When
            let result = buf.accepts(10);

            // Then
            assert_eq!(result, true);
        }

        #[test]
        fn when_buffer_is_flushing() {
            // Given
            let mut buf = LogBuffer::new();
            buf.mark_flush();

            // When
            let result = buf.accepts(BUFFER_SIZE);

            // Then
            assert_eq!(result, false);
        }

        #[test]
        fn when_buffer_does_not_have_enough_space() {
            // Given
            let buf = LogBuffer::new();

            // When
            let result = buf.accepts(BUFFER_SIZE + 1);

            // Then
            assert_eq!(result, false);
        }
    }
}
