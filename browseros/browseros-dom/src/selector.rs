pub fn escape_css_selector(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | '.' | '/'
            | ':' | ';' | '<' | '=' | '>' | '?' | '@' | '[' | '\\' | ']' | '^' | '`' | '{'
            | '|' | '}' | '~' => {
                result.push('\\');
                result.push(ch);
            }
            _ => {
                if ch.is_control() {
                    let code = ch as u32;
                    result.push_str(&format!("\\{code:x} "));
                } else {
                    result.push(ch);
                }
            }
        }
    }
    result
}

pub fn is_valid_css_selector(selector: &str) -> bool {
    !selector.is_empty() && selector.chars().any(|c| !c.is_whitespace())
}

pub fn normalize_selector(selector: &str) -> String {
    selector.split_whitespace().collect::<Vec<&str>>().join(" ")
}
