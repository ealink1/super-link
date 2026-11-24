//! Sha256-based authentication support.
use crate::message::backend::AuthenticationSha256PasswordBody;
use crypto::digest::Digest as sha256Digest;
use crypto::sha2::Sha256;
use lazy_static::lazy_static;
use ring::hmac::Tag;
use ring::hmac::{self};
use ring::pbkdf2::{self, PBKDF2_HMAC_SHA1};
use std::collections::HashMap;
use std::num::NonZeroU32;

lazy_static! {
    static ref HEX_MAP: HashMap<u8, u8> = {
        let mut map: HashMap<u8, u8> = HashMap::with_capacity(30);
        map.insert(48, 0);
        map.insert(49, 1);
        map.insert(50, 2);
        map.insert(51, 3);
        map.insert(52, 4);
        map.insert(53, 5);
        map.insert(54, 6);
        map.insert(55, 7);
        map.insert(56, 8);
        map.insert(57, 9);
        // A - F
        map.insert(65, 10);
        map.insert(66, 11);
        map.insert(67, 12);
        map.insert(68, 13);
        map.insert(69, 14);
        map.insert(70, 15);
        // a - f
        map.insert(97, 10);
        map.insert(98, 11);
        map.insert(99, 12);
        map.insert(100, 13);
        map.insert(101, 14);
        map.insert(102, 15);

        map
    };
}

const LOOKUP_CHAR: [u8; 16] = [
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 98, 99, 100, 101, 102,
];

/// Hashes authentication information in a way suitable for use in response
/// to an `AuthenticationSha256PasswordBody` message.
///
/// The resulting string should be sent back to the database in a
/// `PasswordMessage` message.
#[inline]
pub fn rfc5802_algorithm(password: &[u8], body: AuthenticationSha256PasswordBody) -> Vec<u8> {
    let salt = to_hex_byte(&body.random64code());
    let mut salted_password = [0u8; 32];
    pbkdf2::derive(
        PBKDF2_HMAC_SHA1,
        NonZeroU32::new(body.server_iteration()).unwrap(),
        &salt,
        password,
        &mut salted_password,
    );

    let client_key = get_key_from_hmac(&salted_password, "Client Key".as_bytes());
    let client_key_byte = client_key.as_ref();
    let mut hasher = Sha256::new();
    hasher.input(client_key_byte);
    let mut stored_key: [u8; 32] = [0; 32];
    hasher.result(&mut stored_key);

    let tokenbyte = to_hex_byte(&body.token());

    let hmac_result = get_key_from_hmac(&stored_key, &tokenbyte);
    let h = xor_between_password(hmac_result.as_ref(), client_key_byte, client_key_byte.len());
    bytes_to_hex(&h)
}

/// Convert the numbers to the base 16 chars.
/// for example: 10(0b1010) -> a(97=0b01100001)
///
/// A number will be split to two chars, for example: 26(0b0001_1010) -> 1a
fn bytes_to_hex(h: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::with_capacity(h.len() * 2);
    let mut i = 0;
    while i < h.len() {
        let index = i * 2;
        result.insert(index, LOOKUP_CHAR[(h[i] >> 4) as usize]);
        result.insert(index + 1, LOOKUP_CHAR[(h[i] & 0xF) as usize]);
        i += 1;
    }
    result
}

/// XOR between two passwords
fn xor_between_password(password1: &[u8], password2: &[u8], length: usize) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::with_capacity(length);
    let mut i = 0;
    while i < length {
        result.insert(i, password1[i] ^ password2[i]);
        i += 1;
    }
    result
}

/// SHA256
fn get_key_from_hmac(key: &[u8], data: &[u8]) -> Tag {
    let key2 = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key2, data)
}

/// Convert the base 16 chars to the numbers.
/// for example: a(97=0b01100001) -> 10(0b1010)
///
/// two chars will be merged as a number, for example: 1a -> 26(0b0001_1010)
fn to_hex_byte(hex_char: &[u8]) -> Vec<u8> {
    let mut i = 0;
    let length = hex_char.len() / 2;
    let mut result: Vec<u8> = Vec::with_capacity(length);
    while i < length {
        let index = i * 2;
        result.insert(
            i,
            (HEX_MAP.get(&hex_char[index]).unwrap() << 4)
                | HEX_MAP.get(&hex_char[index + 1]).unwrap(),
        );
        i += 1;
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bytes_to_hex() {
        let salt = [
            0b0000_0001,
            0b0010_0011,
            0b0100_0101,
            0b0110_0111,
            0b1000_1001,
            0b1010_1011,
            0b1100_1101,
            0b1110_1111,
        ];

        let result = bytes_to_hex(&salt);
        assert_eq!(
            std::str::from_utf8(result.as_slice()).unwrap(),
            "0123456789abcdef"
        );
    }

    #[test]
    fn test_rfc5802_algorithm() {
        let password: [u8; 4] = [1, 2, 3, 4];
        let random64code2 = [49; 64];
        let token2 = [51; 8];
        let server_iteration = [0, 0, 0, 1];
        let body = AuthenticationSha256PasswordBody::new(random64code2, token2, server_iteration);
        let result = rfc5802_algorithm(&password, body);
        assert_eq!(
            std::str::from_utf8(result.as_slice()).unwrap(),
            "6308566b6ff5463d5bbcf7cbd95e2bf416a833994ea26d04d48f3b719d4b58fb"
        );
    }

    #[test]
    fn test_to_hex_byte() {
        let param: [u8; 16] = [
            48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 97, 98, 99, 100, 101, 102,
        ];
        let result = to_hex_byte(&param);
        assert_eq!(
            result,
            vec!(
                0b0000_0001,
                0b0010_0011,
                0b0100_0101,
                0b0110_0111,
                0b1000_1001,
                0b1010_1011,
                0b1100_1101,
                0b1110_1111
            )
        );
    }

    #[test]
    fn test_xor_between_password() {
        let password1: [u8; 5] = [
            0b1111_1111,
            0b0000_0000,
            0b0000_0000,
            0b1111_1111,
            0b1111_1111,
        ];
        let password2: [u8; 4] = [0b1111_1111, 0b0000_0000, 0b1111_1111, 0b0000_0000];
        let result = xor_between_password(&password1, &password2, 4);
        assert_eq!(
            result,
            vec!(0b0000_0000, 0b0000_0000, 0b1111_1111, 0b1111_1111)
        );
    }

    #[test]
   fn  test_get_key_from_hmac(){
        let key : [u8;4]=[1,2,3,4] ;
        let data : [u8;4]=[1,2,3,4] ;
        let result = get_key_from_hmac(&key, &data);
        assert_eq!(result.as_ref(), [233, 126, 134, 67, 27, 23, 204, 49, 214, 92, 66, 44, 197, 133, 231, 40, 82, 33, 254, 211, 6, 74, 117, 237, 59, 43, 197, 9, 196, 212, 155, 31]);
    }
}
