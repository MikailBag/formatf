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
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct FormatFnError(FormatError<core::convert::Infallible>, Vec<u8>);

#[cfg(feature = "alloc")]
impl FormatFnError {
    /// Returns reference to underying error
    pub fn error(&self) -> &FormatError<core::convert::Infallible> {
        &self.0
    }

    /// Returns underlying error
    pub fn into_error(self) -> FormatError<core::convert::Infallible> {
        self.0
    }

    /// Returns reference to buffer that was already created when error occured
    pub fn buf(&self) -> &[u8] {
        &self.1
    }

    /// Returns mutable reference to buffer that was already created when error occured.
    /// It can be used to get both error and buffer:
    /// ```
    /// fn on_error(mut err: formatf::FormatFnError) {
    ///     let buf = std::mem::take(err.buf_mut());
    ///     let err = err.into_error();
    ///     // use `buf` and `err` somehow
    /// # drop((buf, err));
    /// }
    /// ```
    pub fn buf_mut(&mut self) -> &mut Vec<u8> {
        &mut self.1
    }

    /// Returns buffer that was already created when error occured
    pub fn into_buf(self) -> Vec<u8> {
        self.1
    }
}

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
    let mut buf = VecSink(Vec::new());
    match format_to(template, args, &mut buf) {
        Ok(()) => Ok(buf.0),
        Err(e) => Err(FormatFnError(e, buf.0)),
    }
}

/// Formats `template` with given `args`, writing to given `sink`
///
/// ```rust,no_run
/// # fn make_sink() -> &'static mut dyn formatf::BinSink<Err=()> {todo!()}
/// use formatf::{format_to, Value};
/// let mut buf = make_sink();
/// format_to(b"Hello, %s!", &[Value::String(b"world")], &mut buf).unwrap();
/// // now buf containes "Hello, world"
/// ```
pub fn format_to<H: BinSink>(
    template: &[u8],
    args: &[Value],
    sink: &mut H,
) -> Result<(), FormatError<H::Err>> {
    let mut fmt = format::Formatter {
        sink,
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
/// In particular, `BinSink` is always implemented for `Vec<u8>` and `&mut [u8]`
pub trait BinSink {
    type Err;
    /// Get and process next chunk of data
    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err>;
}

impl<'a, H: BinSink + ?Sized> BinSink for &'a mut H {
    type Err = <H as BinSink>::Err;
    #[inline]
    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        <H as BinSink>::put(&mut **self, data)
    }
}

/// Helper struct that implements BinSink via delegation to `Write`
#[cfg(feature = "std")]
pub struct WriteSink<W>(W);

#[cfg(feature = "std")]
impl<W: std::io::Write> BinSink for WriteSink<W> {
    type Err = std::io::Error;

    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.0.write_all(data)
    }
}

/// Helper struct that implements BinSink via appending bytes to Vec.
/// This Sink is infallible.
#[cfg(feature = "alloc")]
pub struct VecSink(pub Vec<u8>);

#[cfg(feature = "alloc")]
impl BinSink for VecSink {
    type Err = core::convert::Infallible;

    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.0.extend_from_slice(data);
        Ok(())
    }
}

/// Helper struct that implements BinSink via appending bytes to slice.
/// This Sink only fails when Slice capacity is exceeded
pub struct SliceSink<'a>(pub &'a mut [u8]);

#[derive(Debug)]
pub struct SliceTooSmall;

impl core::fmt::Display for SliceTooSmall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt("slice is too small", f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SliceTooSmall {}

impl<'a> BinSink for SliceSink<'a> {
    type Err = SliceTooSmall;

    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        if self.0.len() < data.len() {
            return Err(SliceTooSmall);
        }
        let (a, b) = core::mem::replace(&mut self.0, &mut []).split_at_mut(data.len());
        a.copy_from_slice(data);
        self.0 = b;
        Ok(())
    }
}

/// Allows define `BinSink`s from functions of form FnMut (u8 slice) -> Result<(), E>)
/// ```rust,no_run
/// use formatf::{CallbackSink, format_to};
/// let mut x = 0u8;
/// let mut cb = |data: &[u8]| {
///     for &byte in data {
///         x ^= byte;
///     }
///     Ok::<_, ()>(())
/// };
/// format_to(b"", &[], &mut CallbackSink::from(cb));
/// ```
pub struct CallbackSink<F>(F);

impl<F> From<F> for CallbackSink<F> {
    fn from(f: F) -> CallbackSink<F> {
        CallbackSink(f)
    }
}

impl<E, F> BinSink for CallbackSink<F>
where
    F: for<'a> FnMut(&'a [u8]) -> Result<(), E>,
{
    type Err = E;
    fn put(&mut self, data: &[u8]) -> Result<(), Self::Err> {
        self.0(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "alloc")]
    fn basic() {
        let res = format(b"He%s=%d", &[Value::String(b"ll"), Value::Int(4)]).unwrap();
        assert_eq!(res, b"Hell=4");
    }

    #[test]
    #[cfg(feature = "alloc")]
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

    #[test]
    #[cfg(feature = "alloc")]
    fn padding_simple_int() {
        let args_small = [Value::Int(42)];
        let args_big = [Value::Int(1234)];
        {
            let fmt = b"%03d";
            let res = format(fmt, &args_small).unwrap();
            assert_eq!(res, b"042");
            let res = format(fmt, &args_big).unwrap();
            assert_eq!(res, b"1234");
        }
        {
            let fmt = b"%3d";
            let res = format(fmt, &args_small).unwrap();
            assert_eq!(res, b" 42");
            let res = format(fmt, &args_big).unwrap();
            assert_eq!(res, b"1234");
        }
    }
}
