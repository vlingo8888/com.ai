
use crate::scan::Span;

#[inline(always)]
pub fn is_valid_before_boundary(c: &u8) -> bool {
    matches!(c.into(), Class::Common | Class::Before)
}

#[inline(always)]
pub fn is_valid_after_boundary(c: &u8) -> bool {
    matches!(c.into(), Class::Common | Class::After)
}

#[inline(always)]
pub fn has_valid_boundaries(span: &Span, input: &[u8]) -> bool {
    let before = {
        if span.start == 0 {
            b'\0'
        } else {
            input[span.start - 1]
        }
    };

    let after = {
        if span.end >= input.len() - 1 {
            b'\0'
        } else {
            input[span.end + 1]
        }
    };

    // Ensure the span has valid boundary characters before and after
    is_valid_before_boundary(&before) && is_valid_after_boundary(&after)
}

#[derive(Debug, Clone, Copy)]
enum Class {
    // Whitespace, e.g.:
    //
    // ```
    // <div class="flex flex-col items-center"></div>
    //                 ^        ^
    // ```
    // Quotes, e.g.:
    //
    // ```
    // <div class="flex">
    //            ^    ^
    // ```
    // End of the input, e.g.:
    //
    // ```
    // flex
    //     ^
    // ```
    Common,

    // Angular like attributes, e.g.:
    //
    // ````
    // [class.foo]
    //       ^
    // ```
    // Twig-like templating languages, e.g.:
    //
    // ```
    // <div class="{% if true %}flex{% else %}block{% endif %}">
    //                         ^
    // ```
    // XML-like languages where classes are inside the tag, e.g.:
    // ```
    // <f:case value="0">from-blue-900 to-cyan-200</f:case>
    //                  ^
    // ```
    Before,

    // Clojure and Angular like languages, e.g.:
    // ```
    // [:div.p-2]
    //          ^
    // [class.foo]
    //           ^
    // ```
    // Twig like templating languages, e.g.:
    //
    // ```
    // <div class="{% if true %}flex{% else %}block{% endif %}">
    //                              ^
    // ```
    // Svelte like attributes, e.g.:
    //
    // ```
    // <div class:flex="bool"></div>
    //                ^
    // ```
    // Escaped character when embedding one language in another via strings, e.g.:
    //
    // ```
    // $attributes->merge([
    //   'x-init' => '$el.classList.add(\'-translate-x-full\'); $el.classList.add(\'transition-transform\')',
    //                                                     ^                                            ^
    // ]);
    // ```
    //
    // In this case there is some JavaScript embedded in an string in PHP and some of the quotes
    // need to be escaped.
    // XML-like languages where classes are inside the tag, e.g.:
    // ```
    // <f:case value="0">from-blue-900 to-cyan-200</f:case>
    //                                            ^
    // ```
    After,

    Other,
}
impl Class {
    pub const TABLE: [Class; 256] = {
        let mut t = [Class::Other; 256];
        t[b'\t' as usize] = Class::Common;
        t[b'\n' as usize] = Class::Common;
        t[b'\x0C' as usize] = Class::Common;
        t[b'\r' as usize] = Class::Common;
        t[b' ' as usize] = Class::Common;
        t[b'"' as usize] = Class::Common;
        t[b'\'' as usize] = Class::Common;
        t[b'`' as usize] = Class::Common;
        t[b'\0' as usize] = Class::Common;
        t[b'.' as usize] = Class::Before;
        t[b'}' as usize] = Class::Before;
        t[b'>' as usize] = Class::Before;
        t[b']' as usize] = Class::After;
        t[b'{' as usize] = Class::After;
        t[b'=' as usize] = Class::After;
        t[b'\\' as usize] = Class::After;
        t[b'<' as usize] = Class::After;
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
