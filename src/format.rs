use crate::{
    high::{ConvKind, LenModifier, ParseError, ParsedConversionSpecification},
    visit::{ConversionSpecification, FormatStringVisitor},
    Handler, Value,
};

#[derive(Debug)]
pub enum FormatError<E> {
    /// Handler returned error.
    Handler(E),
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

/// Formatter actually implements formatting
pub(crate) struct Formatter<'a, H: Handler> {
    pub(crate) handler: &'a mut H,
    pub(crate) args: &'a [Value<'a>],
    pub(crate) error: Option<FormatError<H::Err>>,
    pub(crate) next_arg: usize,
}

impl<'a, H: Handler> Formatter<'a, H> {
    fn had_error(&self) -> bool {
        self.error.is_some()
    }

    #[must_use]
    fn call_handler(&mut self, b: &[u8]) -> bool {
        match self.handler.handle(b) {
            Ok(()) => true,
            Err(e) => {
                self.error = Some(FormatError::Handler(e));
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

        let pad_char = b' ';

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
                self.error = Some(FormatError::BadType);
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
                        self.error = Some(FormatError::Unsupported);
                        return;
                    }
                };

                if x < low_bound || up_bound < x {
                    self.error = Some(FormatError::NumOverflow);
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
                self.error = Some(FormatError::BadType);
                return;
            }
            ConvKind::String => {
                if spec.flags.alt
                    || spec.flags.pad_zero
                    || spec.flags.comma_groups
                    || spec.flags.alt_digits
                {
                    self.error = Some(FormatError::Invalid);
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
            self.error = Some(FormatError::NotEnoughArguments);
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

impl<'a, H: Handler> FormatStringVisitor for Formatter<'a, H> {
    fn visit_bytes(&mut self, b: &[u8]) {
        if self.had_error() {
            return;
        }
        if let Err(e) = self.handler.handle(b) {
            self.error = Some(FormatError::Handler(e));
        }
    }

    fn visit_specification(&mut self, spec: ConversionSpecification) {
        if self.had_error() {
            return;
        }
        let hi_spec = match ParsedConversionSpecification::try_parse(spec) {
            Ok(x) => x,
            Err(e) => {
                self.error = Some(FormatError::Spec(e));
                return;
            }
        };
        if !hi_spec.is_supported() {
            self.error = Some(FormatError::Unsupported);
            return;
        }
        self.format(hi_spec);
    }
}
