//! MTEXT formatting-code removal.

use std::iter::Peekable;
use std::str::Chars;

use crate::codepage::unescape;

const ARGUMENT_COMMANDS: &str = "fFHhCcWwQqAaTt";
const TOGGLE_COMMANDS: &str = "LlOoKk";
const NON_BREAKING_SPACE: char = '\u{a0}';

/// Removes braces and formatting commands from MTEXT contents, renders stacked fractions as `a/b`, then expands text escapes.
pub fn strip_mtext(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(next) = chars.next() {
        match next {
            '{' | '}' => {}
            '\\' => command(&mut chars, &mut out),
            other => out.push(other),
        }
    }
    unescape(&out)
}

fn command(chars: &mut Peekable<Chars>, out: &mut String) {
    let Some(letter) = chars.next() else {
        out.push('\\');
        return;
    };
    match letter {
        '\\' | '{' | '}' => out.push(letter),
        'P' => out.push('\n'),
        '~' => out.push(NON_BREAKING_SPACE),
        'S' => stack(&argument(chars), out),
        toggle if TOGGLE_COMMANDS.contains(toggle) => {}
        with_argument if ARGUMENT_COMMANDS.contains(with_argument) => {
            argument(chars);
        }
        other => {
            out.push('\\');
            out.push(other);
        }
    }
}

fn argument(chars: &mut Peekable<Chars>) -> String {
    let mut argument = String::new();
    while let Some(&next) = chars.peek() {
        if next == '\\' {
            break;
        }
        chars.next();
        if next == ';' {
            break;
        }
        argument.push(next);
    }
    argument
}

fn stack(argument: &str, out: &mut String) {
    let Some((numerator, denominator)) = argument.split_once(['^', '/', '#']) else {
        out.push_str(argument);
        return;
    };
    out.push_str(numerator);
    out.push('/');
    out.push_str(denominator);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_format_codes_and_keeps_text() {
        assert_eq!(
            strip_mtext("{\\fArial|b1;Hello} \\PWorld %%d"),
            "Hello \nWorld \u{b0}"
        );
    }

    #[test]
    fn unbalanced_braces_still_yield_text() {
        assert_eq!(strip_mtext("{\\fArial|b1; unterminated"), " unterminated");
    }

    #[test]
    fn stacking_renders_as_a_slash_fraction() {
        assert_eq!(strip_mtext("\\S1^2;"), "1/2");
        assert_eq!(strip_mtext("x\\S3/4;y"), "x3/4y");
        assert_eq!(strip_mtext("\\S5#8;"), "5/8");
    }

    #[test]
    fn escaped_braces_and_backslashes_survive() {
        assert_eq!(strip_mtext("\\{a\\}\\\\b"), "{a}\\b");
    }

    #[test]
    fn nonbreaking_space_and_paragraph_breaks() {
        assert_eq!(strip_mtext("a\\~b\\Pc"), "a\u{a0}b\nc");
    }

    #[test]
    fn unterminated_command_ends_at_the_next_backslash() {
        assert_eq!(strip_mtext("\\H2.5x\\Pnext"), "\nnext");
    }

    #[test]
    fn unicode_escapes_pass_through_to_unescape() {
        assert_eq!(strip_mtext("\\U+00B0"), "\u{b0}");
    }
}
