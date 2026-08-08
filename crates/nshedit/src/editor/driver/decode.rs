use std::collections::VecDeque;

use crate::domain::TextUnit;

#[derive(Debug, Default)]
pub(super) struct Decoder {
    pending: Vec<u8>,
    ready: VecDeque<TextUnit>,
}

impl Decoder {
    pub(super) fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        self.decode(false);
    }

    pub(super) fn finish(&mut self) {
        self.decode(true);
    }

    pub(super) fn pop(&mut self) -> Option<TextUnit> {
        self.ready.pop_front()
    }

    pub(super) fn clear(&mut self) {
        self.pending.clear();
        self.ready.clear();
    }

    fn decode(&mut self, final_input: bool) {
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    self.ready.extend(text.chars().map(TextUnit::Scalar));
                    self.pending.clear();
                    return;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    let scalars: Vec<_> = std::str::from_utf8(&self.pending[..valid])
                        .expect("valid_up_to identifies valid UTF-8")
                        .chars()
                        .map(TextUnit::Scalar)
                        .collect();
                    self.ready.extend(scalars);
                    self.pending.drain(..valid);

                    match error.error_len() {
                        Some(length) => {
                            self.ready
                                .extend(self.pending.drain(..length).map(TextUnit::RawByte));
                        }
                        None if final_input => {
                            self.ready
                                .extend(self.pending.drain(..).map(TextUnit::RawByte));
                            return;
                        }
                        None => return,
                    }
                }
            }
        }
    }
}
