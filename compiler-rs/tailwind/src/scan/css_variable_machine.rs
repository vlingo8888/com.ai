use crate::scan::cursor;
use crate::scan::machine::{Machine, MachineState};

/// Extract CSS variables from an input.
///
/// E.g.:
///
/// ```text
/// var(--my-variable)
///     ^^^^^^^^^^^^^
/// ```
#[derive(Debug, Default)]
pub struct CssVariableMachine;

impl Machine for CssVariableMachine {
    #[inline(always)]
    fn reset(&mut self) {}

    #[inline]
    fn next(&mut self, cursor: &mut cursor::Cursor<'_>) -> MachineState {
        // CSS Variables must start with `--`
        if Class::Dash != cursor.curr().into() || Class::Dash != cursor.next().into() {
            return MachineState::Idle;
        }

        let start_pos = cursor.pos;
        let len = cursor.input.len();

        cursor.advance_twice();

        while cursor.pos < len {
            match cursor.curr().into() {
                // https://drafts.csswg.org/css-syntax-3/#ident-token-diagram
                //
                Class::AllowedCharacter | Class::Dash => {
                    match cursor.next().into() {
                        // Valid character followed by a valid character or an escape character
                        //
                        // E.g.: `--my-variable`
                        //                ^^
                        // E.g.: `--my-\#variable`
                        //            ^^
                        Class::AllowedCharacter | Class::Dash | Class::Escape => cursor.advance(),

                        // Valid character followed by anything else means the variable is done
                        //
                        // E.g.: `'--my-variable'`
                        //                      ^
                        _ => {
                            // There must be at least 1 character after the `--`
                            if cursor.pos - start_pos < 2 {
                                return self.restart();
                            } else {
                                return self.done(start_pos, cursor);
                            }
                        }
                    }
                }

                Class::Escape => match cursor.next().into() {
                    // An escaped whitespace character is not allowed
                    //
                    // In CSS it is allowed, but in the context of a class it's not because then we
                    // would have spaces in the class.
                    //
                    // E.g.: `bg-(--my-\ color)`
                    //                  ^
                    Class::Whitespace => return self.restart(),

                    // An escape at the end of the class is not allowed
                    Class::End => return self.restart(),

                    // An escaped character, skip the next character, resume after
                    //
                    // E.g.: `--my-\#variable`
                    //             ^           We are here
                    //               ^         Resume here
                    _ => cursor.advance_twice(),
                },

                // Character is not valid anymore
                _ => return self.restart(),
            }
        }

        MachineState::Idle
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Dash,

    // non-ASCII (such as Emoji): https://drafts.csswg.org/css-syntax-3/#non-ascii-ident-code-point
    AllowedCharacter,

    Escape,

    Whitespace,

    End,

    Other,
}
impl Class {
    pub const TABLE: [Class; 256] = {
        let mut t = [Class::Other; 256];
        t[b'-' as usize] = Class::Dash;
        t[b'_' as usize] = Class::AllowedCharacter;
        { let mut b = b'a' as usize; while b <= b'z' as usize { t[b] = Class::AllowedCharacter; b += 1; } }
        { let mut b = b'A' as usize; while b <= b'Z' as usize { t[b] = Class::AllowedCharacter; b += 1; } }
        { let mut b = b'0' as usize; while b <= b'9' as usize { t[b] = Class::AllowedCharacter; b += 1; } }
        { let mut b = 0x80 as usize; while b <= 0xff as usize { t[b] = Class::AllowedCharacter; b += 1; } }
        t[b'\\' as usize] = Class::Escape;
        t[b' ' as usize] = Class::Whitespace;
        t[b'\t' as usize] = Class::Whitespace;
        t[b'\n' as usize] = Class::Whitespace;
        t[b'\r' as usize] = Class::Whitespace;
        t[b'\x0C' as usize] = Class::Whitespace;
        t[b'\0' as usize] = Class::End;
        t
    };
}

impl From<u8> for Class {
    #[inline(always)]
    fn from(byte: u8) -> Self { Class::TABLE[byte as usize] }
}

impl From<&u8> for Class {
    #[inline(always)]
    fn from(byte: &u8) -> Self { Class::TABLE[*byte as usize] }
}

#[cfg(test)]
mod tests {
    use super::CssVariableMachine;
    use crate::scan::machine::Machine;

    #[test]
    #[ignore]
    fn test_css_variable_machine_performance() {
        let input = r#"This sentence will contain a few variables here and there var(--my-variable) --other-variable-1\/2 var(--more-variables-here)"#.repeat(100);

        CssVariableMachine::test_throughput(100_000, &input);
        CssVariableMachine::test_duration_once(&input);

        todo!();
    }

    #[test]
    fn test_css_variable_machine_extraction() {
        for (input, expected) in [
            // Simple variable
            ("--foo", vec!["--foo"]),
            ("--my-variable", vec!["--my-variable"]),
            // Multiple variables
            (
                "calc(var(--first) + var(--second))",
                vec!["--first", "--second"],
            ),
            // Variables with... emojis
            ("--😀", vec!["--😀"]),
            ("--😀-😁", vec!["--😀-😁"]),
            // Escaped character in the middle, skips the next character
            (r#"--spacing-1\/2"#, vec![r#"--spacing-1\/2"#]),
            // Escaped whitespace is not allowed
            (r#"--my-\ variable"#, vec![]),
            // --------------------------
            //
            // Exceptions
            // Not a valid variable
            ("", vec![]),
            ("-", vec![]),
            ("--", vec![]),
        ] {
            for wrapper in [
                // No wrapper
                "{}",
                // With leading spaces
                " {}",
                // With trailing spaces
                "{} ",
                // Surrounded by spaces
                " {} ",
                // Inside a string
                "'{}'",
                // Inside a function call
                "fn({})",
                // Inside nested function calls
                "fn1(fn2({}))",
                // --------------------------
                //
                // HTML
                // Inside a class (on its own)
                r#"<div class="{}"></div>"#,
                // Inside a class (first)
                r#"<div class="{} foo"></div>"#,
                // Inside a class (second)
                r#"<div class="foo {}"></div>"#,
                // Inside a class (surrounded)
                r#"<div class="foo {} bar"></div>"#,
                // Inside an arbitrary property
                r#"<div class="[{}:red]"></div>"#,
                // --------------------------
                //
                // JavaScript
                // Inside a variable
                r#"let classes = '{}';"#,
                // Inside an object (key)
                r#"let classes = { '{}': true };"#,
                // Inside an object (no spaces, key)
                r#"let classes = {'{}':true};"#,
                // Inside an object (value)
                r#"let classes = { primary: '{}' };"#,
                // Inside an object (no spaces, value)
                r#"let classes = {primary:'{}'};"#,
                // Inside an array
                r#"let classes = ['{}'];"#,
            ] {
                let input = wrapper.replace("{}", input);

                let actual = CssVariableMachine::test_extract_all(&input);

                if actual != expected {
                    dbg!(&input);
                }

                assert_eq!(actual, expected);
            }
        }
    }
}
