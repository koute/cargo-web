/// Returns a cross-platform-filename-safe version of any string.
///
/// This is used internally to generate app data directories based on app
/// name/author. App developers can use it for consistency when dealing with
/// file system operations.
///
/// Do not apply this function to full paths, as it will sanitize '/' and '\';
/// it should only be used on directory or file names (i.e. path segments).
pub fn sanitized(component: &str) -> String {
    let mut buf = String::with_capacity(component.len());
    for (i, c) in component.chars().enumerate() {
        let is_lower = 'a' <= c && c <= 'z';
        let is_upper = 'A' <= c && c <= 'Z';
        let is_letter = is_upper || is_lower;
        let is_number = '0' <= c && c <= '9';
        let is_space = c == ' ';
        let is_hyphen = c == '-';
        let is_underscore = c == '_';
        let is_period = c == '.' && i != 0; // Disallow accidentally hidden folders
        let is_valid = is_letter || is_number || is_space || is_hyphen || is_underscore ||
                       is_period;
        if is_valid {
            buf.push(c);
        } else {
            buf.push_str(&format!(",{},", c as u32));
        }
    }
    buf
}
