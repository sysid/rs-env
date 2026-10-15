//! Terminal output formatting with colors
//!
//! Respects NO_COLOR, CLICOLOR, CLICOLOR_FORCE automatically.

use colored::Colorize;

/// Print error (red bold "error:" prefix) to stderr
pub fn error(msg: &(impl std::fmt::Display + ?Sized)) {
    eprintln!("{}: {}", "error".red().bold(), msg);
}

/// Print warning (yellow "Warning:" prefix) to stderr
pub fn warning(msg: &(impl std::fmt::Display + ?Sized)) {
    eprintln!("{}: {}", "Warning".yellow(), msg);
}

/// Print success status (green checkmark)
pub fn success(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("{} {}", "✓".green(), msg);
}

/// Print success status indented (green checkmark with leading spaces)
pub fn success_detail(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("  {} {}", "✓".green(), msg);
}

/// Print failure status (red X, indented)
pub fn failure(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("  {} {}", "✗".red(), msg);
}

/// Print completed action (green label)
pub fn action(label: &str, msg: &(impl std::fmt::Display + ?Sized)) {
    println!("{}: {}", label.green(), msg);
}

/// Print section header (cyan bold)
pub fn header(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("{}", msg.to_string().cyan().bold());
}

/// Print diff addition (green +)
pub fn diff_add(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("  {} {}", "+".green(), msg);
}

/// Print diff removal (red -)
pub fn diff_remove(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("  {} {}", "-".red(), msg);
}

/// Print indented detail (no color)
pub fn detail(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("  {}", msg);
}

/// Print plain output (no color, for data/export statements)
pub fn info(msg: &(impl std::fmt::Display + ?Sized)) {
    println!("{}", msg);
}

/// Print prompt without newline (cyan)
pub fn prompt(msg: &(impl std::fmt::Display + ?Sized)) {
    use std::io::Write;
    print!("{} ", msg.to_string().cyan());
    std::io::stdout().flush().ok();
}

/// Ask a yes/no question. Only `y`/`Y` confirms, so Enter, anything else, or EOF
/// (non-interactive stdin) aborts - the safe default for destructive commands.
pub fn confirm(msg: &(impl std::fmt::Display + ?Sized)) -> std::io::Result<bool> {
    prompt(msg);
    confirm_from(&mut std::io::stdin().lock())
}

fn confirm_from(input: &mut impl std::io::BufRead) -> std::io::Result<bool> {
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("y"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_y_in_any_case_when_confirm_then_true() {
        assert!(confirm_from(&mut "y\n".as_bytes()).unwrap());
        assert!(confirm_from(&mut " Y \n".as_bytes()).unwrap());
    }

    #[test]
    fn given_enter_other_answer_or_eof_when_confirm_then_false() {
        assert!(!confirm_from(&mut "\n".as_bytes()).unwrap());
        assert!(!confirm_from(&mut "yes\n".as_bytes()).unwrap());
        assert!(!confirm_from(&mut "".as_bytes()).unwrap());
    }
}
