#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum Error {
    /// XCB error as a result of an X request.
    #[display("{}", _0)]
    #[from(xcb::Error, xcb::ConnError, xcb::ProtocolError)]
    XcbError(xcb::Error),
    /// Unrecognised screen number, i.e. negative or does not match any existing
    /// screen.
    #[display("invalid screen number: {}", _0)]
    BadScreenNumber(i32),
    /// Display server uses unsupported visual.  This library currently supports
    /// only 24 or 32-bit TrueColour visual.
    #[display("unsupported visual class: {}-bit {:?}", _0, _1)]
    UnsupportedVisual(u8, xcb::x::VisualClass),
    /// Failed to locate visual that matches the root visual.
    #[display("could not find root visual: {}", _0)]
    CouldNotFindRootVisual(xcb::x::Visualid),
    /// Image too large.  Image dimensions must fit 16-bit unsigned integer.
    #[display("image {}x{} too large", _0, _1)]
    ImageTooLarge(u32, u32),
    /// The image buffer size does not match image dimensions.
    #[display("buffer {} does not match {}x{} image size", _0, _1, _2)]
    BadBufferSize(usize, u16, u16),
}


/// Unrecognised screen number, i.e. negative or does not match any existing
/// screen.
#[derive(Debug, derive_more::Display)]
#[display("invalid screen number: {}", _0)]
pub struct BadScreenNumber(pub i32);

impl From<BadScreenNumber> for Error {
    fn from(err: BadScreenNumber) -> Error { Error::BadScreenNumber(err.0) }
}
