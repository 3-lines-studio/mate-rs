pub(crate) fn truncate_with_ellipsis(s: &str, max_len: usize, ellipsis: &str) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let cut = max_len.saturating_sub(ellipsis.len());
    let boundary = s
        .char_indices()
        .take_while(|&(i, _)| i <= cut)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}{}", &s[..boundary], ellipsis)
}
