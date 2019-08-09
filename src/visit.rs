//! Simplifies custom work with format strings
//!

/// Represents all parts of conversion specification
///
/// Following applies for slices:
/// - Slices are taker from original string
/// - They do not overlap, but consecutive slices touch themselves
/// - Next slice is located after previous
///
/// Note that each slice can be empty
pub struct ConversionSpecification<'a> {
    pub flags: &'a [u8],
    pub field_width: &'a [u8],
    pub precision: &'a [u8],
    pub length: &'a [u8],
    pub specifier: &'a [u8],
}

pub trait FormatStringVisitor {
    /// String chunk, taken from format string as is
    fn visit_bytes(&mut self, b: &[u8]) {
        let _ = b;
    }

    /// Called when escaped percent (`%%`) is found
    fn visit_escaped_percent(&mut self) {
        self.visit_bytes(b"%");
    }

    /// Called when conversion specification is found
    fn visit_specification(&mut self, spec: ConversionSpecification) {
        let _ = spec;
    }

    /// Called on format string finish
    fn visit_eof(&mut self) {}
}

impl<T: FormatStringVisitor> FormatStringVisitor for &mut T {
    fn visit_bytes(&mut self, b: &[u8]) {
        (**self).visit_bytes(b);
    }

    fn visit_escaped_percent(&mut self) {
        (**self).visit_escaped_percent();
    }

    fn visit_specification(&mut self, spec: ConversionSpecification) {
        (**self).visit_specification(spec);
    }

    fn visit_eof(&mut self) {
        (**self).visit_eof();
    }
}

pub fn visit(s: &[u8], vis: impl FormatStringVisitor) {
    crate::parser::do_visit(s, vis);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq, Default)]
    pub struct OwnedConversionSpecification {
        pub flags: Vec<u8>,
        pub field_width: Vec<u8>,
        pub precision: Vec<u8>,
        pub length: Vec<u8>,
        pub specifier: Vec<u8>,
    }

    impl<'a> From<ConversionSpecification<'a>> for OwnedConversionSpecification {
        fn from(x: ConversionSpecification<'a>) -> Self {
            OwnedConversionSpecification {
                flags: x.flags.to_vec(),
                field_width: x.field_width.to_vec(),
                precision: x.precision.to_vec(),
                length: x.length.to_vec(),
                specifier: x.specifier.to_vec(),
            }
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    enum Event {
        String(Vec<u8>),
        Percent,
        ConvSpec(OwnedConversionSpecification),
    }

    #[derive(Default)]
    struct CollectVisitor {
        data: Vec<Event>,
    }

    impl FormatStringVisitor for CollectVisitor {
        fn visit_bytes(&mut self, b: &[u8]) {
            self.data.push(Event::String(b.to_vec()))
        }

        fn visit_escaped_percent(&mut self) {
            self.data.push(Event::Percent)
        }

        fn visit_specification(&mut self, spec: ConversionSpecification) {
            self.data.push(Event::ConvSpec(spec.into()))
        }
    }

    fn visit_to_events(s: &[u8]) -> Vec<Event> {
        let mut vis: CollectVisitor = Default::default();
        visit(s, &mut vis);
        vis.data
    }

    mod parsing {
        use super::{visit_to_events, Event, OwnedConversionSpecification};

        fn check(s: &[u8], expected: &[Event]) {
            let actual = visit_to_events(s);
            assert_eq!(actual, expected);
        }

        #[test]
        fn string_only() {
            check(b"Hello world", &[Event::String(b"Hello world".to_vec())]);
        }

        #[test]
        fn simple_spec_only() {
            check(
                b"%d",
                &[Event::ConvSpec(OwnedConversionSpecification {
                    specifier: b"d".to_vec(),
                    ..Default::default()
                })],
            )
        }

        #[test]
        fn escape_percents() {
            check(b"%%%%%%", &[Event::Percent, Event::Percent, Event::Percent])
        }

        #[test]
        fn some_spec() {
            check(
                b"Hell%o, %%%% %#I02.4Lf worl%d",
                &[
                    Event::String(b"Hell".to_vec()),
                    Event::ConvSpec(OwnedConversionSpecification {
                        specifier: b"o".to_vec(),
                        ..Default::default()
                    }),
                    Event::String(b", ".to_vec()),
                    Event::Percent,
                    Event::Percent,
                    Event::String(b" ".to_vec()),
                    Event::ConvSpec(OwnedConversionSpecification {
                        flags: b"#I0".to_vec(),
                        field_width: b"2".to_vec(),
                        precision: b".4".to_vec(),
                        length: b"L".to_vec(),
                        specifier: b"f".to_vec(),
                    }),
                    Event::String(b" worl".to_vec()),
                    Event::ConvSpec(OwnedConversionSpecification {
                        specifier: b"d".to_vec(),
                        ..Default::default()
                    }),
                ],
            )
        }
    }
}
