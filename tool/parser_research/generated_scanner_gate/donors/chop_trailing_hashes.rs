pub fn chop_trailing_hashes(mut line: &str) -> (&str, bool) {
    line = rtrim_slice(line);

    let orig_n = line.len() - 1;
    let mut n = orig_n;

    let bytes = line.as_bytes();
    while bytes[n] == b'#' {
        if n == 0 {
            return (line, false);
        }
        n -= 1;
    }

    if n != orig_n && is_space_or_tab(bytes[n]) {
        (rtrim_slice(&line[..n]), true)
    } else {
        (line, false)
    }
}
