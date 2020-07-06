use crate::{
    high::{ConvKind, LenModifier, ParseError, ParsedConversionSpecification},
    visit::{ConversionSpecification, FormatStringVisitor},
    BinSink, Value,
};

#[derive(Debug)]
pub enum FormatToError<E> {
    /// `BinSink` returned error.
    Sink(E),
    /// Conversion specifier parse error.
    Spec(ParseError),
    /// Too many arguments.
    ///
    /// All processing was done.
    /// I.e., you can ignore this error to get printf-like behavior
    ExcessArgs,
    /// Not enough arguments.
    NotEnoughArguments,
    /// Format string contains unsupported feature.
    Unsupported,
    /// Type mismatch.
    BadType,
    /// Invalid combination of flags, modifiers, etc was encountered. Returned in situations
    /// where `printf` behavior would be undefined.
    Invalid,
    /// Value passed was out of numeric limits for conversion requested.
    NumOverflow,
}

impl<E> FormatToError<E> {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Sink(_) => "sink error",
            Self::Spec(_) => "invalid conversion specifier",
            Self::ExcessArgs => "format string did not use all given args",
            Self::NotEnoughArguments => {
                "format string requested arguments that were not provided"
            }
            Self::Unsupported => "this feature is not implemented yet",
            Self::BadType => "argument type not compatible with conversion specifier",
            Self::Invalid => "invalid format string",
            Self::NumOverflow => "numeric overflow",
        }
    }
}

impl<E: core::fmt::Display> core::fmt::Display for FormatToError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        std::fmt::Display::fmt(self.description(), f)
    }
}

#[cfg(feature = "std")]
impl<E: std::error::Error + 'static> std::error::Error for FormatToError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sink(inner) => Some(inner),
            Self::Spec(inner) => Some(inner),
            _ => None,
        }
    }
}

/// Formatter actually implements formatting
pub(crate) struct Formatter<'a, H: BinSink> {
    pub(crate) sink: &'a mut H,
    pub(crate) args: &'a [Value<'a>],
    pub(crate) error: Option<FormatToError<H::Err>>,
    pub(crate) next_arg: usize,
}

impl<'a, H: BinSink> Formatter<'a, H> {
    fn had_error(&self) -> bool {
        self.error.is_some()
    }

    #[must_use]
    fn call_handler(&mut self, b: &[u8]) -> bool {
        match self.sink.put(b) {
            Ok(()) => true,
            Err(e) => {
                self.error = Some(FormatToError::Sink(e));
                false
            }
        }
    }

    #[must_use]
    fn write_padding(&mut self, c: u8, cnt: usize) -> bool {
        debug_assert!(c == b'0' || c == b' ');
        let sl = &[c];
        for _ in 0..cnt {
            if !self.call_handler(sl) {
                return false;
            }
        }
        true
    }

    /// helper for format_* methods to deal with padding
    fn write_data(&mut self, data: &[u8], spec: ParsedConversionSpecification) {
        let padding_size = if data.len() < spec.min_width {
            spec.min_width - data.len()
        } else {
            0
        };

        let pad_char = if spec.flags.pad_zero { b'0' } else { b' ' };

        if !spec.flags.adj_left {
            if !self.write_padding(pad_char, padding_size) {
                return;
            }
        }

        if !self.call_handler(data) {
            return;
        }

        if spec.flags.adj_left {
            if !self.write_padding(pad_char, padding_size) {
                return;
            }
        }
    }

    fn format_int(&mut self, x: i128, spec: ParsedConversionSpecification) {
        match spec.conv_kind {
            ConvKind::String => {
                self.error = Some(FormatToError::BadType);
                return;
            }
            ConvKind::SignDecInt => {
                // check limits
                let (low_bound, up_bound) = match spec.len_modifier {
                    LenModifier::Shorter => (i8::min_value() as i128, i8::max_value() as i128),
                    LenModifier::Short => (i16::min_value() as i128, i16::max_value() as i128),
                    LenModifier::None => (i32::min_value() as i128, i32::max_value() as i128),
                    LenModifier::Long | LenModifier::Longer => {
                        (i64::min_value() as i128, i64::max_value() as i128)
                    }
                    LenModifier::Longest => (i128::min_value(), i128::max_value()),
                    LenModifier::Size => (isize::min_value() as i128, isize::max_value() as i128),
                    _ => {
                        self.error = Some(FormatToError::Unsupported);
                        return;
                    }
                };

                if x < low_bound || up_bound < x {
                    self.error = Some(FormatToError::NumOverflow);
                    return;
                }

                let mut buf = itoa::Buffer::new();
                let data = buf.format(x).as_bytes();
                self.write_data(data, spec);
            }
        }
    }

    fn format_bytes(&mut self, b: &[u8], spec: ParsedConversionSpecification) {
        match spec.conv_kind {
            ConvKind::SignDecInt => {
                self.error = Some(FormatToError::BadType);
                return;
            }
            ConvKind::String => {
                if spec.flags.alt
                    || spec.flags.pad_zero
                    || spec.flags.comma_groups
                    || spec.flags.alt_digits
                {
                    self.error = Some(FormatToError::Invalid);
                    return;
                }
                let prec = spec.prec.unwrap_or(b.len());

                let write_part = &b[..std::cmp::min(b.len(), prec)];
                self.write_data(write_part, spec);
            }
        }
    }

    fn format(&mut self, spec: ParsedConversionSpecification) {
        if self.next_arg == self.args.len() {
            self.error = Some(FormatToError::NotEnoughArguments);
            return;
        }
        let arg = &self.args[self.next_arg];
        self.next_arg += 1;
        match *arg {
            Value::Int(x) => self.format_int(x, spec),
            Value::String(bytes) => self.format_bytes(bytes, spec),
        }
    }
}

impl<'a, H: BinSink> FormatStringVisitor for Formatter<'a, H> {
    fn visit_bytes(&mut self, b: &[u8]) {
        if self.had_error() {
            return;
        }
        if let Err(e) = self.sink.put(b) {
            self.error = Some(FormatToError::Sink(e));
        }
    }

    fn visit_specification(&mut self, spec: ConversionSpecification) {
        if self.had_error() {
            return;
        }
        let hi_spec = match ParsedConversionSpecification::try_parse(spec) {
            Ok(x) => x,
            Err(e) => {
                self.error = Some(FormatToError::Spec(e));
                return;
            }
        };
        if !hi_spec.is_supported() {
            self.error = Some(FormatToError::Unsupported);
            return;
        }
        self.format(hi_spec);
    }
}
