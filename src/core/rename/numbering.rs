use super::*;

pub(crate) fn apply_numbering(
    s: &str,
    mode: NumberingMode,
    num: isize,
    pad: usize,
    cfg: &NumberingConfig,
) -> String {
    let num_str = format_number(num, pad, cfg.num_type, cfg.num_case);
    let sep = cfg.sep.unwrap_or("");
    match mode {
        NumberingMode::Prefix => format!("{}{}{}", num_str, sep, s),
        NumberingMode::Suffix => format!("{}{}{}", s, sep, num_str),
        NumberingMode::Both => format!("{}{}{}{}{}", num_str, sep, s, sep, num_str),
        NumberingMode::Insert => {
            if let Some(p) = cfg.pos {
                apply_insert(s, &format!("{}{}", num_str, sep), p)
            } else {
                format!("{}{}{}", num_str, sep, s)
            }
        }
    }
}

pub(crate) fn number_to_base(num: usize, base: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if num == 0 {
        return "0".to_string();
    }
    let base = base.clamp(2, 36);
    let mut n = num;
    let mut result = Vec::new();
    while n > 0 {
        result.push(DIGITS[(n as u32 % base) as usize]);
        n /= base as usize;
    }
    result.reverse();
    String::from_utf8(result).unwrap()
}

pub(crate) fn number_to_alpha(mut num: usize, uppercase: bool) -> String {
    let mut result = Vec::new();
    while num > 0 {
        num -= 1;
        let rem = (num % 26) as u8;
        result.push(if uppercase { b'A' + rem } else { b'a' + rem });
        num /= 26;
    }
    result.reverse();
    if result.is_empty() {
        return (if uppercase { "A" } else { "a" }).to_string();
    }
    String::from_utf8(result).unwrap()
}

pub(crate) fn number_to_roman(mut num: usize) -> String {
    const VALUES: [(usize, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut result = String::new();
    for &(v, n) in &VALUES {
        while num >= v {
            result.push_str(n);
            num -= v;
        }
    }
    result
}

pub(crate) fn format_number(
    num: isize,
    pad: usize,
    num_type: Option<&str>,
    num_case: Option<&str>,
) -> String {
    if num < 0 {
        // For negative numbers, just format with sign for decimal, fallback to
        // positive representation for non-decimal types
        if num_type.is_none()
            || num_type == Some("decimal")
            || num_type == Some("")
            || num_type == Some("10")
        {
            let abs = (-num) as usize;
            return format!("-{:01$}", abs, pad);
        }
        // Non-decimal types with negative numbers: use absolute value
        let abs = (-num) as usize;
        return format_number(abs as isize, pad, num_type, num_case);
    }
    let raw = match num_type {
        Some(t) if t == "hex" || t == "16" => format!("{:x}", num as usize),
        Some(t) if t == "octal" || t == "8" => format!("{:o}", num as usize),
        Some("az_upper") => number_to_alpha(num as usize, true),
        Some("az_lower") | Some("alpha") => number_to_alpha(num as usize, false),
        Some("roman") => number_to_roman(num as usize),
        Some(base_str) if !base_str.is_empty() && base_str != "decimal" => {
            if let Ok(base) = base_str.parse::<u32>() {
                if (2..=36).contains(&base) {
                    number_to_base(num as usize, base)
                } else {
                    num.to_string()
                }
            } else {
                num.to_string()
            }
        }
        _ => return format!("{:01$}", num, pad),
    };
    let raw = match num_case {
        Some("upper") => raw.to_uppercase(),
        Some("lower") => raw.to_lowercase(),
        _ => raw,
    };
    if pad > 0 {
        format!("{:0>width$}", raw, width = pad)
    } else {
        raw
    }
}
