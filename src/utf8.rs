use std::fmt;

#[derive(Debug)]
pub enum ModifiedUtf8Error {
    Incomplete,
    Malformed,
}

impl std::error::Error for ModifiedUtf8Error {}

impl fmt::Display for ModifiedUtf8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "utf8_modified {:?}", self)
    }
}
/// Decode a Modified UTF-8 encoded string.
///
/// This encoding is used by Java for fast serialization of its strings, and
/// has some minor deviations from standard UTF-8.
pub fn from_modified_utf8(mut bytes: &[u8]) -> Result<String, ModifiedUtf8Error> {
    // Preallocate the biggest buffer we might need
    let mut string = String::with_capacity(bytes.len() - 1);

    loop {
        // Treat it as normal UTF-8 - since the encodings are so similar, we can
        // reuse the original agorithm
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                string.push_str(s);
                // We've reached the end of the string
                return Ok(string);
            }
            Err(e) => {
                string.push_str(unsafe {
                    // SAFETY: https://doc.rust-lang.org/std/str/struct.Utf8Error.html#method.valid_up_to
                    std::str::from_utf8_unchecked(bytes.get_unchecked(..e.valid_up_to()))
                });
                // We have encountered some bytes that aren't UTF-8. If they're valid
                // Modified UTF-8, decode them
                match unsafe { bytes.get_unchecked(e.valid_up_to()..) } {
                    [0b1100_0000, 0b1000_0000, rest @ ..] => {
                        string.push_str("\0");
                        bytes = rest
                    }
                    // The surrogate code units match the bit mask 0b1101_1yxx_xxxx_xxxx,
                    // where `y` is high for the high surrogate and vice versa.
                    // Encoded in UTF-8, that's 0b1110_1101 0b101y_xxxx 0b10xx_xxxx.
                    // They must come as a (high, low) pair, so we can match on the
                    // whole pattern.
                    [
                        0b1110_1101,
                        second @ 0b1010_0000..=0b1010_1111,
                        third @ 0b1000_0000..=0b1011_1111,
                        0b1110_1101,
                        fifth @ 0b1011_0000..=0b1011_1111,
                        sixth @ 0b1000_0000..=0b1011_1111,
                        rest @ ..
                    ] => {
                        // Decode from UTF-8
                        let high_surrogate =
                            (((second & 0b0000_1111) as u32) << 6) | (third & 0b0011_1111) as u32;
                        let low_surrogate =
                            (((fifth & 0b0000_1111) as u32) << 6) | (sixth & 0b0011_1111) as u32;
                        let chr = unsafe {
                            // Decode the surrogate pair.
                            // SAFETY: Each surrogate is in the range 0-0x3FF (10 bits wide), and the
                            // maximum value of 0x10FFFF (0x10000 + 0x3FF * 2^10 + 0x3FF) lands this
                            // within the Supplementary Code Point range (0x10000-0x10FFFF)
                            std::char::from_u32_unchecked(
                                0x10000 + ((high_surrogate << 10) | low_surrogate),
                            )
                        };
                        let mut buf = [0; 4];
                        string.push_str(chr.encode_utf8(&mut buf));
                        bytes = rest;
                    }
                    [] => return Err(ModifiedUtf8Error::Incomplete),
                    [..] => return Err(ModifiedUtf8Error::Malformed),
                };
            }
        }
    }
}
