//! High-level utilities for work with [`ConversionSpecification`]
//!
//! [`ConversionSpecification`]: ../visit/struct.ConversionSpecification.html

use crate::visit::ConversionSpecification;

/// Represents various errors that can occur while parsing ConversionSpecification.
///
/// Note that many variants can only happen
/// if you are using hand-crafted `ConversionSpecification`.
#[derive(Debug)]
pub enum ParseError {
    MissingSpecifier,
    UnknownSpecifier,
    UnknownFlag(u8),
    DuplicateFlag(u8),
    /// Precision is invalid
    InvalidPrec(Option<core::num::ParseIntError>),
    InvalidWidth(Option<core::num::ParseIntError>),
    UnknownLenModifier,
    Unsupported,
}

impl ParseError {
    pub fn description(&self) -> &'static str {
        match self {
            ParseError::MissingSpecifier => "conversion specifier missing",
            ParseError::UnknownSpecifier => "unknown conversion specifier",
            ParseError::UnknownFlag(_) => "conversion specifier contains unknown flag",
            ParseError::DuplicateFlag(_) => "conversion specifier contains duplicated flag",
            ParseError::InvalidPrec(_) => "conversion specifier contains invalid precision",
            ParseError::InvalidWidth(_) => "conversion specifier contains invalid width",
            ParseError::UnknownLenModifier => {
                "conversion specifier contains unknown length modifier"
            }
            ParseError::Unsupported => "this feature is not supported",
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self.description(), f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::InvalidPrec(inner) | ParseError::InvalidWidth(inner) => inner
                .as_ref()
                .map(|x| x as &(dyn std::error::Error + 'static)),
            _ => None,
        }
    }
}

impl ParsedConversionSpecification {
    pub fn try_parse(
        spec: ConversionSpecification,
    ) -> Result<ParsedConversionSpecification, ParseError> {
        let specifier = ConvKind::from_bytes(spec.specifier).ok_or_else(|| {
            if spec.specifier.is_empty() {
                ParseError::MissingSpecifier
            } else {
                ParseError::UnknownSpecifier
            }
        })?;
        let flags = ConvFlags::from_bytes(spec.flags)?;

        let len_mod = LenModifier::from_bytes(spec.length)?;

        let min_width = {
            if spec.field_width.is_empty() {
                0
            } else {
                std::str::from_utf8(spec.field_width)
                    .map_err(|_| ParseError::InvalidWidth(None))
                    .and_then(|s| s.parse().map_err(|e| ParseError::InvalidWidth(Some(e))))?
            }
        };

        let prec = {
            if spec.precision.is_empty() {
                None
            } else {
                if spec.precision[0] != b'.' {
                    return Err(ParseError::InvalidPrec(None));
                }
                let prec = &spec.precision[1..];
                let p = std::str::from_utf8(prec)
                    .map_err(|_| ParseError::InvalidPrec(None))
                    .and_then(|s| s.parse().map_err(|e| ParseError::InvalidPrec(Some(e))))?;
                Some(p)
            }
        };

        Ok(ParsedConversionSpecification {
            conv_kind: specifier,
            len_modifier: len_mod,
            min_width,
            prec,
            flags,
        })
    }

    pub(crate) fn is_supported(&self) -> bool {
        self.flags.is_supported()
    }
}

/// Conversion specifier
pub enum ConvKind {
    SignDecInt,
    String,
}

impl ConvKind {
    fn from_bytes(b: &[u8]) -> Option<Self> {
        use ConvKind::*;
        match b {
            b"d" | b"i" => Some(SignDecInt),
            b"s" => Some(String),
            _ => None,
        }
    }
}

/// Length modifier
pub enum LenModifier {
    None,
    /// Corresponds to `l`.
    Long,
    /// Corresponds to `ll`.
    Longer,
    /// Corresponds to `j`.
    Longest,
    /// Corresponds to `h`.
    Short,
    /// Corresponds to `hh`.
    Shorter,
    /// Corresponds to `L`.
    LongDouble,
    /// Corresponds to `z`.
    Size,
    /// Corresponds to `t`.
    PtrDiff,
}

impl LenModifier {
    pub fn from_bytes(b: &[u8]) -> Result<LenModifier, ParseError> {
        use LenModifier::*;
        match b {
            b"l" => Ok(Long),
            b"ll" => Ok(Longer),
            b"j" => Ok(Longest),
            b"h" => Ok(Short),
            b"hh" => Ok(Shorter),
            b"L" => Ok(LongDouble),
            b"z" | b"Z" => Ok(Size),
            b"t" => Ok(PtrDiff),
            b"" => Ok(None),
            _ => Err(ParseError::UnknownLenModifier),
        }
    }
}

/// Conversion flags
#[derive(Default)]
pub struct ConvFlags {
    /// `ConvKind`-dependent Alternate representation. Corresponds to `#`.
    pub alt: bool,
    /// Zero-padding. Corresponds to `0`.
    pub pad_zero: bool,
    /// Adjust to left. Corresponds to `-`.
    pub adj_left: bool,
    /// Produce whitespace before positive integer. Corresponds to ` `.
    pub pos_space: bool,
    /// Always put sign before integer. Corresponds to `+`.
    pub force_sign: bool,
    /// Group thousands using comma. Corresponds to `'`
    pub comma_groups: bool,
    /// Use locale-alternative output digits. Corresponds to `I`.
    pub alt_digits: bool,
}

impl ConvFlags {
    fn from_bytes(b: &[u8]) -> Result<Self, ParseError> {
        let mut flags = ConvFlags::default();
        for &ch in b {
            let field = match ch {
                b'#' => &mut flags.alt,
                b'0' => &mut flags.pad_zero,
                b'-' => &mut flags.adj_left,
                b' ' => &mut flags.pos_space,
                b'+' => &mut flags.force_sign,
                b'\'' => &mut flags.comma_groups,
                b'I' => &mut flags.alt_digits,
                _ => return Err(ParseError::UnknownFlag(ch)),
            };
            if *field {
                return Err(ParseError::DuplicateFlag(ch));
            }
            *field = true;
        }
        Ok(flags)
    }

    /// Checks that all triggered flags are supported by `formatf`
    fn is_supported(&self) -> bool {
        !self.alt_digits && !self.comma_groups
    }
}

/// Utility struct, which parses conversion specification according to `man`
pub struct ParsedConversionSpecification {
    pub conv_kind: ConvKind,
    pub flags: ConvFlags,
    pub len_modifier: LenModifier,
    pub min_width: usize,
    pub prec: Option<usize>,
}
