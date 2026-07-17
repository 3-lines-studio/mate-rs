use std::time::Duration;

pub fn format_duration(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        let ms = d.as_millis() as u64;
        return format!("{}ms", ms);
    }
    let total_ns = d.as_nanos();
    let rounded_ns = total_ns / 10_000_000 * 10_000_000;
    let rounded = Duration::from_nanos(rounded_ns as u64);
    let ns = rounded.as_nanos();
    let hour_ns: u128 = 3600 * 1_000_000_000;
    let min_ns: u128 = 60 * 1_000_000_000;
    let sec_ns: u128 = 1_000_000_000;
    if ns >= hour_ns {
        let h = ns / hour_ns;
        let rem = ns % hour_ns;
        let m = rem / min_ns;
        let rem2 = rem % min_ns;
        let s = rem2 / sec_ns;
        return format!("{}h{}m{}s", h, m, s);
    }
    if ns >= min_ns {
        let m = ns / min_ns;
        let rem = ns % min_ns;
        if rem == 0 {
            return format!("{}m0s", m);
        }
        let s = rem / sec_ns;
        let frac_ns = rem % sec_ns;
        let sec_str = fmt_sec_fractional(s, frac_ns);
        return format!("{}m{}s", m, sec_str);
    }
    let s = ns / sec_ns;
    let frac_ns = ns % sec_ns;
    if frac_ns == 0 {
        format!("{}s", s)
    } else {
        format!("{}s", fmt_sec_fractional(s, frac_ns))
    }
}

fn fmt_sec_fractional(secs: u128, frac_ns: u128) -> String {
    let ds = format!("{:09}", frac_ns);
    let trimmed = ds.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{}", secs)
    } else {
        format!("{}.{}", secs, trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration_500ms() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
    }

    #[test]
    fn test_format_duration_1s() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1s");
    }

    #[test]
    fn test_format_duration_1500ms() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
    }

    #[test]
    fn test_format_duration_125s340ms() {
        let d = Duration::from_secs(125) + Duration::from_millis(340);
        assert_eq!(format_duration(d), "2m5.34s");
    }
}
