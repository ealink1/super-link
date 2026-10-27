//! Minimal standard-alphabet Base64, mirroring Go's `encoding/base64`
//! `StdEncoding`: padding is required and CR/LF are ignored while decoding.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (first << 16) | (second << 8) | third;
        output.push(ALPHABET[(triple >> 18) as usize & 63] as char);
        output.push(ALPHABET[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(triple >> 6) as usize & 63] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[triple as usize & 63] as char);
        } else {
            output.push('=');
        }
    }
    output
}

pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let filtered: Vec<u8> = text
        .bytes()
        .filter(|byte| *byte != b'\r' && *byte != b'\n')
        .collect();
    if filtered.len() % 4 != 0 {
        return None;
    }
    let mut output = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks(4) {
        let mut values = [0u8; 4];
        let mut padding = 0usize;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                if index < 2 {
                    return None;
                }
                padding += 1;
                continue;
            }
            if padding > 0 {
                return None;
            }
            values[index] = decode_char(*byte)?;
        }
        let triple = ((values[0] as u32) << 18)
            | ((values[1] as u32) << 12)
            | ((values[2] as u32) << 6)
            | values[3] as u32;
        output.push((triple >> 16) as u8);
        if padding < 2 {
            output.push((triple >> 8) as u8);
        }
        if padding < 1 {
            output.push(triple as u8);
        }
    }
    Some(output)
}

fn decode_char(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_binary_payloads() {
        for payload in [
            b"".as_slice(),
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"\x00\xff\x10",
        ] {
            let text = encode(payload);
            assert_eq!(decode(&text).as_deref(), Some(payload), "{text}");
        }
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(encode(b"fo"), "Zm8=");
    }

    #[test]
    fn rejects_malformed_padding() {
        assert!(decode("Zm8").is_none());
        assert!(decode("Z===").is_none());
        assert!(decode("Zm=v").is_none());
        assert!(decode("!!!!").is_none());
        assert_eq!(decode("Zm8=\n").as_deref(), Some(b"fo".as_slice()));
    }
}
