//! Basic lexing
//! Refer to `man printf(3)` for details about format string format

use crate::visit::{ConversionSpecification, FormatStringVisitor};

/// Conversion specification
/// Fields contain position in string, where corresponding item begins
#[derive(Copy, Clone, Debug)]
struct RawSpec {
    flags: usize,
    field_width: usize,
    precision: usize,
    length: usize,
    /// Conversion specifier
    conv_spec: usize,
    end: usize,
}

#[derive(Copy, Clone, Debug)]
enum State {
    /// Entered at beginning, and when Spec or %% is parsed
    None,
    /// Will be provided as is. Begin position is attached
    String(usize),
    /// Percent was received
    Percent,
}

fn is_flag(c: u8) -> bool {
    b"#0+- +'I".contains(&c)
}

fn is_conversion_specifier(c: u8) -> bool {
    b"diouxXeEfFgGaAcsCSpnm".contains(&c)
}

fn is_length_modifier(c: u8) -> bool {
    b"hlqLjzZt".contains(&c)
}

pub(crate) fn do_visit(s: &[u8], mut vis: impl FormatStringVisitor) {
    let mut state = State::None;

    let n = s.len();
    // index of next char to look at
    let mut i = 0;
    while i < n {
        let on_percent = s[i] == b'%';
        match state {
            State::None => {
                if on_percent {
                    state = State::Percent;
                } else {
                    state = State::String(i);
                }
            }
            State::String(j) => {
                if on_percent {
                    vis.visit_bytes(&s[j..i]);
                    state = State::Percent;
                }
                // else - do nothing, string continues
            }
            State::Percent => {
                if on_percent {
                    // this is just escaped percent
                    vis.visit_escaped_percent();
                    state = State::None;
                } else {
                    // most complex part - here we want to eat complete conversion specification

                    /* TODO: We want parse spec as loosely as possible, to allow user define
                       custom flags, specifiers etc. This isn't easy task, because of ambiguity
                       between flags and length modifiers, so probably some ParseConfig struct
                       should be provided
                    */
                    let mut spec = RawSpec {
                        flags: 0,
                        field_width: 0,
                        precision: 0,
                        length: 0,
                        conv_spec: 0,
                        end: 0,
                    };
                    spec.flags = i;
                    i -= 1;
                    loop {
                        i += 1;
                        let ch = if i < n {
                            s[i]
                        } else {
                            // this allows easily handle EOF:
                            // \0 will be recognized as conversion specification end
                            b'\0'
                        };
                        let maybe_flag = is_flag(ch);
                        let maybe_field_width = ch.is_ascii_digit();
                        let maybe_precision = ch == b'.' || ch.is_ascii_digit();
                        let maybe_length_modifier = is_length_modifier(ch);
                        let maybe_conv_spec = is_conversion_specifier(ch);
                        if spec.field_width == 0 {
                            // we are still parsing flags
                            if maybe_flag {
                                // ok, continue eating flag
                                continue;
                            } else {
                                spec.field_width = i;
                                // flags finished
                            }
                        }
                        if spec.precision == 0 {
                            // we are parsing field width
                            if maybe_field_width {
                                // ok, continue eating field width
                                continue;
                            } else {
                                spec.precision = i;
                                // field width finished
                            }
                        }
                        if spec.length == 0 {
                            // we are parsing precision
                            if maybe_precision {
                                // ok, continue eating precision
                                continue;
                            } else {
                                spec.length = i;
                                // precision finished
                            }
                        }
                        if spec.conv_spec == 0 {
                            // we are parsing length modifier
                            if maybe_length_modifier {
                                // ok, continue eating length
                                continue;
                            } else {
                                spec.conv_spec = i;
                            }
                        }
                        if spec.end == 0 {
                            // we are parsing conversion specifier
                            if maybe_conv_spec {
                                // ok, continue eating conversion specifier
                                continue;
                            } else {
                                spec.end = i;
                                break;
                            }
                        }
                    }
                    let vis_spec = ConversionSpecification {
                        flags: &s[spec.flags..spec.field_width],
                        field_width: &s[spec.field_width..spec.precision],
                        precision: &s[spec.precision..spec.length],
                        length: &s[spec.length..spec.conv_spec],
                        specifier: &s[spec.conv_spec..spec.end],
                    };
                    vis.visit_specification(vis_spec);
                    state = if i == n {
                        State::None
                    } else {
                        State::String(i)
                    }
                }
            }
        }

        i += 1;
    }
    if let State::String(j) = state {
        vis.visit_bytes(&s[j..]);
    }
}
