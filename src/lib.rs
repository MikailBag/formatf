//! `formatf` allows you to format strings at runtime
//!
//! # Features
//! - format!()-like functionality - create new String with rendered template (requires `alloc` feature)
//! - write!()-like functionality - write template to custom callback or `std::io::Write` implementation (latter requires `std` feature)
//! - no_std / alloc-only support
//! - does not depend on any C libraries
//! - safe panic-free API - if incorrect format string is provided, Error will returned instead of panic of triggering UB
//! - bytes-oriented - format string must not be utf8
//! - alloc-efficient - allocations are avoided if possible
//! # Notes
//!

#[cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
mod std {
    use core::*;
}

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use crate::format::FormatError;

mod format;
pub mod high;
mod parser;
pub mod visit;

pub enum Value<'a> {
    Int(i128),
    String(&'a [u8]),
}

/// Error returned by [`format`]
///
/// [`format`]: ./fn.format.html
#[derive(Debug)]
pub struct FormatFnError(FormatError<<Vec<u8> as Handler>::Err>, Vec<u8>);

/// Formats `template` with given `args` to new-allocated buf
///
/// Simple example:
/// ```rust
/// use formatf::{format, Value};
/// let buf = format(b"Hello, world. My integer is %d", &[Value::Int(42)]).unwrap();
/// assert_eq!(buf, b"Hello, world. My integer is 42");
/// ```
#[cfg(feature = "alloc")]
pub fn format(template: &[u8], args: &[Value]) -> Result<Vec<u8>, FormatFnError> {
    let mut buf = Vec::new();
    match format_to(template, args, &mut buf) {
        Ok(()) => Ok(buf),
        Err(e) => Err(FormatFnError(e, buf)),
    }
}

/// Formats `template` with given `args`, writing to given `handler`
///
/// ```rust
/// use formatf::{format_to, Value};
/// let mut buf = Vec::with_capacity(13);
/// format_to(b"Hello, %s!", &[Value::String(b"world")], &mut buf).unwrap();
/// assert_eq!(buf, b"Hello, world!");
/// assert_eq!(buf.capacity(), 13);
/// ```
pub fn format_to<H: Handler>(
    template: &[u8],
    args: &[Value],
    handler: &mut H,
) -> Result<(), FormatError<H::Err>> {
    let mut fmt = format::Formatter {
        handler,
        args,
        error: None,
        next_arg: 0,
    };
    visit::visit(template, &mut fmt);
    match fmt.error.ok_or(()) {
        Ok(err) => Err(err),
        Err(ok) => Ok(ok),
    }
}

/// This is something like `std::io::Write`, but with `no_std` support.
///
/// In particular, `Handler` is always implemented for `Vec<u8>` and `&mut [u8]`
pub trait Handler {
    type Err;
    fn handle(&mut self, data: &[u8]) -> Result<(), Self::Err>;
}

#[cfg(feature = "std")]
impl<W: std::io::Write> Handler for W {
    type Err = std::io::Error;

    fn handle(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.write_all(data)
    }
}

#[cfg(not(feature = "std"))]
impl Handler for Vec<u8> {
    type Err = !;

    fn handle(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.extend_from_slice(data);
        Ok(())
    }
}

#[cfg(not(feature = "std"))]
impl<'a> Handler for &'a mut [u8] {
    type Err = ();

    fn handle(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        if self.len() < data.len() {
            return Err(());
        }
        let (a, b) = self.split_at_mut(data.len());
        a.copy_from_slice(data);
        *self = b;
        Ok(())
    }
}

/// Allows define `Handler`s from functions of form FnMut (u8 slice) -> Result<(), E>)
/// ```rust,no_run
/// use formatf::{CallbackHandler, format_to};
/// let mut x = 0u8;
/// let mut cb = |data: &[u8]| {
///     for &byte in data {
///         x ^= byte;
///     }
///     Ok::<_, ()>(())
/// };
/// format_to(b"", &[], &mut CallbackHandler::from(cb));
/// ```
pub struct CallbackHandler<F>(F);

impl<F> From<F> for CallbackHandler<F> {
    fn from(f: F) -> CallbackHandler<F> {
        CallbackHandler(f)
    }
}

impl<E, F> Handler for CallbackHandler<F>
where
    F: for<'a> FnMut(&'a [u8]) -> Result<(), E>,
{
    type Err = E;
    fn handle(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.0(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        let res = format(b"He%s=%d", &[Value::String(b"ll"), Value::Int(4)]).unwrap();
        assert_eq!(res, b"Hell=4");
    }

    #[test]
    fn padding_simple_string() {
        let args_hi = [Value::String(b"hi")];
        let args_loop = [Value::String(b"loop")];
        {
            let fmt = b"%3s";
            let res = format(fmt, &args_hi).unwrap();
            assert_eq!(res, b" hi");
            let res = format(fmt, &args_loop).unwrap();
            assert_eq!(res, b"loop");
        }
        {
            let fmt = b"%-3s";
            let res = format(fmt, &args_hi).unwrap();
            assert_eq!(res, b"hi ");
            let res = format(fmt, &args_loop).unwrap();
            assert_eq!(res, b"loop");
        }
    }
}
