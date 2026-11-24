//! Exact PostgreSQL-compatible binary NUMERIC arguments without float conversion.
pub fn encode(text: &str) -> Result<Vec<u8>, String> {
    if text.len() > 65536 {
        return Err("数值参数过长".into());
    }
    let negative = text.starts_with('-');
    let text = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    if integer.is_empty() && fraction.is_empty()
        || !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err("十进制参数无效".into());
    }
    let groups = integer.len().div_ceil(4);
    let left = (4 - integer.len() % 4) % 4;
    let right = (4 - fraction.len() % 4) % 4;
    let joined = format!(
        "{}{}{}{}",
        "0".repeat(left),
        integer,
        fraction,
        "0".repeat(right)
    );
    let mut digits = joined
        .as_bytes()
        .chunks(4)
        .map(|b| std::str::from_utf8(b).unwrap().parse::<u16>().unwrap())
        .collect::<Vec<_>>();
    let mut weight = groups as i16 - 1;
    while digits.first() == Some(&0) {
        digits.remove(0);
        weight -= 1;
    }
    while digits.last() == Some(&0) {
        digits.pop();
    }
    let mut bytes = Vec::new();
    for n in [
        digits.len() as u16,
        weight as u16,
        if negative { 0x4000 } else { 0 },
        fraction.len() as u16,
    ] {
        bytes.extend(n.to_be_bytes());
    }
    for n in digits {
        bytes.extend(n.to_be_bytes());
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precision_and_scale_roundtrip() {
        for value in [
            "12345678901234567890.123456789",
            "-0.0000123400",
            "0",
            "42.0000",
        ] {
            let encoded = encode(value).unwrap();
            assert_eq!(
                superlink_drivers::native::postgres::decode_numeric(&encoded).unwrap(),
                value
            );
        }
        assert!(encode("1e9").is_err());
    }
}
