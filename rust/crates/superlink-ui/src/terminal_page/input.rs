//! xterm keyboard sequences, including application cursor and modified keys.
pub(super) fn encode(
    key: &str,
    control: bool,
    shift: bool,
    alt: bool,
    application: bool,
) -> Option<Vec<u8>> {
    if control {
        let byte = match key {
            "space" | "@" => Some(0),
            "\\" => Some(28),
            "]" => Some(29),
            "^" => Some(30),
            "_" | "-" => Some(31),
            "?" => Some(127),
            _ => None,
        };
        if let Some(byte) = byte {
            return Some(vec![byte]);
        }
    }
    if key == "tab" && shift {
        return Some(b"\x1b[Z".to_vec());
    }
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    let suffix = match key {
        "up" => Some('A'),
        "down" => Some('B'),
        "right" => Some('C'),
        "left" => Some('D'),
        "home" => Some('H'),
        "end" => Some('F'),
        _ => None,
    };
    if let Some(suffix) = suffix {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{suffix}")
            } else if application {
                format!("\x1bO{suffix}")
            } else {
                format!("\x1b[{suffix}")
            }
            .into_bytes(),
        );
    }
    let function = match key {
        "f1" => Some('P'),
        "f2" => Some('Q'),
        "f3" => Some('R'),
        "f4" => Some('S'),
        _ => None,
    };
    if let Some(suffix) = function {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{suffix}")
            } else {
                format!("\x1bO{suffix}")
            }
            .into_bytes(),
        );
    }
    let number = match key {
        "insert" => Some(2),
        "delete" => Some(3),
        "pageup" => Some(5),
        "pagedown" => Some(6),
        "f5" => Some(15),
        "f6" => Some(17),
        "f7" => Some(18),
        "f8" => Some(19),
        "f9" => Some(20),
        "f10" => Some(21),
        "f11" => Some(23),
        "f12" => Some(24),
        _ => None,
    };
    if let Some(number) = number {
        return Some(
            if modifier > 1 {
                format!("\x1b[{number};{modifier}~")
            } else {
                format!("\x1b[{number}~")
            }
            .into_bytes(),
        );
    }
    super::encode_key(key, control)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_screen_programs_receive_application_and_modified_keys() {
        assert_eq!(encode("up", false, false, false, true).unwrap(), b"\x1bOA");
        assert_eq!(
            encode("left", true, false, false, false).unwrap(),
            b"\x1b[1;5D"
        );
        assert_eq!(encode("tab", false, true, false, false).unwrap(), b"\x1b[Z");
        assert_eq!(
            encode("f12", false, false, false, false).unwrap(),
            b"\x1b[24~"
        );
        assert_eq!(encode("space", true, false, false, false).unwrap(), [0]);
        assert!(encode("中", false, false, false, false).is_none());
    }
}
