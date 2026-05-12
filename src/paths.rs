use std::path::PathBuf;

pub fn home() -> PathBuf {
    dirs::home_dir().expect("HOME must be set")
}

pub fn config_dir() -> PathBuf {
    home().join(".config/claude-sandbox")
}

pub fn data_dir() -> PathBuf {
    home().join(".local/share/claude-sandbox")
}

pub fn cache_dir() -> PathBuf {
    home().join(".cache/claude-sandbox")
}

pub fn expand(input: &str) -> String {
    let mut s = input.to_string();
    if let Some(rest) = s.strip_prefix("~/") {
        s = home().join(rest).display().to_string();
    } else if s == "~" {
        s = home().display().to_string();
    }
    expand_env(&s)
}

fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > i + 1 {
                let key = std::str::from_utf8(&bytes[i + 1..end]).unwrap();
                if let Ok(v) = std::env::var(key) {
                    out.push_str(&v);
                    i = end;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
