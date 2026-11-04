//! Human-readable formatting for SDK data. This does not redact sensitive fields.

use std::{ffi::c_char, fmt};

struct CharacterCodes<'a>(&'a [c_char]);

impl fmt::Debug for CharacterCodes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|&value| value as u8 as char))
            .finish()
    }
}

struct Bytes<'a>(&'a [c_char]);

impl fmt::Debug for Bytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|&value| value as u8))
            .finish()
    }
}

fn bytes_with_length(value: &[c_char], length: i32) -> &[c_char] {
    // Invalid SDK lengths retain the complete buffer for diagnosis, without panicking.
    value
        .get(..usize::try_from(length).unwrap_or(usize::MAX))
        .unwrap_or(value)
}
