#[derive(Debug, derive_more::Display, derive_more::From)]
#[non_exhaustive]
pub enum Error {
    /// XCB error as a result of an X request.
    #[display("{}", _0)]
    #[from(xcb::Error, xcb::ConnError, xcb::ProtocolError)]
    Xcb(xcb::Error),
    /// Unrecognised screen number, i.e. negative or does not match any existing
    /// screen.
    #[display("invalid screen number: {}", _0)]
    BadScreenNumber(i32),
    /// Display server uses unsupported visual.  This library supports 24- or
    /// 32-bit TrueColour visual only.
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
    #[cfg(feature = "image")]
    #[display("{}", _0)]
    #[from]
    Imgage(image::error::ImageError),
}


/// Unrecognised screen number, i.e. negative or does not match any existing
/// screen.
#[derive(Debug, PartialEq, Eq, derive_more::Display)]
#[display("invalid screen number: {}", _0)]
pub struct BadScreenNumber(pub i32);

impl From<BadScreenNumber> for Error {
    fn from(err: BadScreenNumber) -> Error { Error::BadScreenNumber(err.0) }
}


/// The image dimensions are too large to fit `u16`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, derive_more::Display)]
#[display("image too large: {}x{}", _0, _1)]
pub struct ImageTooLarge(pub u32, pub u32);

impl From<ImageTooLarge> for Error {
    fn from(err: ImageTooLarge) -> Error { Error::ImageTooLarge(err.0, err.1) }
}


/// The image buffer size does not match image dimensions.
#[derive(Debug, derive_more::Display)]
#[display("buffer {} does not match {}x{} image size", _0, _1, _2)]
pub struct BadBufferSize(pub usize, pub u16, pub u16);

impl From<BadBufferSize> for Error {
    fn from(err: BadBufferSize) -> Error {
        Error::BadBufferSize(err.0, err.1, err.2)
    }
}


impl std::error::Error for Error {}
impl std::error::Error for BadScreenNumber {}
impl std::error::Error for BadBufferSize {}
