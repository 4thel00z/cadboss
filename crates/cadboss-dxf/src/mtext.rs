//! MTEXT formatting-code removal.

use crate::codepage::{backslash_escape, unescape_percent};

const ARGUMENT_COMMANDS: &str = "fFHhCcWwQqAaTtp";
const TOGGLE_COMMANDS: &str = "LlOoKk";
const ARGUMENT_END: [char; 4] = [';', '\\', '{', '}'];
const NON_BREAKING_SPACE: char = '\u{a0}';

/// Removes braces and formatting commands from MTEXT contents and renders stacked fractions as `a/b`.
/// Paragraph breaks (`\P`), column breaks (`\N`) and the dimension-line break (`\X`) become newlines.
/// Backslash escapes are resolved here once, so an escaped backslash is never rescanned; percent codes are expanded last.
pub fn strip_mtext(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(next) = rest.chars().next() {
        if next == '\\' {
            rest = command(rest, &mut out);
            continue;
        }
        if !matches!(next, '{' | '}') {
            out.push(next);
        }
        rest = rest.get(next.len_utf8()..).unwrap_or("");
    }
    unescape_percent(&out)
}

fn command<'a>(rest: &'a str, out: &mut String) -> &'a str {
    if let Some((replacement, used)) = backslash_escape(rest) {
        out.extend(replacement);
        return rest.get(used..).unwrap_or("");
    }
    let Some(letter) = rest.get(1..).and_then(|after| after.chars().next()) else {
        out.push('\\');
        return "";
    };
    let after = rest.get(1 + letter.len_utf8()..).unwrap_or("");
    match letter {
        '\\' | '{' | '}' => out.push(letter),
        'P' | 'N' | 'X' => out.push('\n'),
        '~' => out.push(NON_BREAKING_SPACE),
        'S' => {
            let (argument, remaining) = argument(after);
            stack(argument, out);
            return remaining;
        }
        toggle if TOGGLE_COMMANDS.contains(toggle) => {}
        with_argument if ARGUMENT_COMMANDS.contains(with_argument) => {
            return argument(after).1;
        }
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    after
}

fn argument(after: &str) -> (&str, &str) {
    let Some(end) = after.find(ARGUMENT_END) else {
        return (after, "");
    };
    let argument = after.get(..end).unwrap_or("");
    let skip = if after.get(end..).is_some_and(|tail| tail.starts_with(';')) {
        end + 1
    } else {
        end
    };
    (argument, after.get(skip..).unwrap_or(""))
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

    #[test]
    fn paragraph_codes_are_removed() {
        assert_eq!(strip_mtext("\\pxqc;Title"), "Title");
        assert_eq!(strip_mtext("\\pi-3,l3;Item"), "Item");
    }

    #[test]
    fn escaped_backslash_is_not_rescanned_as_a_unicode_escape() {
        assert_eq!(strip_mtext("\\\\U+00B0"), "\\U+00B0");
        assert_eq!(strip_mtext("\\U+00B0"), "\u{b0}");
    }

    #[test]
    fn column_and_dimension_line_breaks_become_newlines() {
        assert_eq!(strip_mtext("Line 1\\NLine 2"), "Line 1\nLine 2");
        assert_eq!(strip_mtext("upper\\Xlower"), "upper\nlower");
    }

    #[test]
    fn unterminated_argument_stops_at_a_closing_brace() {
        assert_eq!(strip_mtext("{\\fArial Hello}world"), "world");
        assert_eq!(strip_mtext("{\\fArial|b1;Hello}world"), "Helloworld");
    }
}
