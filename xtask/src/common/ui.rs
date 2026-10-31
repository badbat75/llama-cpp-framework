//! Console output: the colours the scripts have always used, only when
//! stdout is a terminal.

use std::io::IsTerminal;

fn paint(code: &str, s: &str) -> String {
    if std::io::stdout().is_terminal() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn say(s: impl AsRef<str>) {
    println!("{}", s.as_ref());
}
pub fn head(s: impl AsRef<str>) {
    println!("{}", paint("36", s.as_ref()));
}
pub fn ok(s: impl AsRef<str>) {
    println!("{}", paint("32", s.as_ref()));
}
pub fn warn(s: impl AsRef<str>) {
    println!("{}", paint("33", s.as_ref()));
}
pub fn err(s: impl AsRef<str>) {
    eprintln!("{}", paint("31", s.as_ref()));
}
pub fn dim(s: impl AsRef<str>) {
    println!("{}", paint("90", s.as_ref()));
}
