/// Definition of a pixel format used by X display server.
///
/// The pixel format is defined as shift values for red, green and blue subpixel
/// value.  The colour is represented by a `u32` value whose component channels
/// are 8-bit values shifted to the left by corresponding shift values.
///
/// Common format for colour is `0x00_RR_GG_BB` which is described as 16, 8 and
/// 0 shifts for red, green and blue colour components respectively.  Beware
/// that on little endian systems (so practically on all systems), such colour
/// is encoded as `[red, green, blue, 0]` bytes in memory.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::Debug,
)]
#[display("RgbShifts({}, {}, {})", r, g, b)]
#[debug("{}", self)]
pub struct RgbShifts {
    /// Bit shift value for red component in `u32` colour description.
    /// Typically 16.
    pub r: u8,
    /// Bit shift value for red component in `u32` colour description.
    /// Typically 8.
    pub g: u8,
    /// Bit shift value for red component in `u32` colour description.
    /// Typically 0.
    pub b: u8,
}

impl RgbShifts {
    /// Constructs a colour representation from red, green and blue components.
    ///
    /// ```
    /// let shifts = setroot::img::RgbShifts { r: 16, g: 8, b: 0 }
    /// assert_eq!(0x00_FF_F8_E7, shifts.from_rgb(0xFF, 0xF8, 0xE7));
    ///
    /// let colour = shifts.from_rgb(1, 2, 3);
    /// assert_eq!(0x00_01_02_03, colour);
    /// if cfg!(target_endian = "little") {
    ///     assert_eq!([3, 2, 1, 0], colour.to_le_bytes());
    /// } else {
    ///     assert_eq!([0, 1, 2, 3], colour.to_le_bytes());
    /// }
    /// ```
    pub fn from_rgb<S: Subpixel>(&self, r: S, g: S, b: S) -> u32 {
        (u32::from(r.to_u8()) << self.r) |
            (u32::from(g.to_u8()) << self.g) |
            (u32::from(b.to_u8()) << self.b)
    }

    /// Constructs a greyscale colour representation from luma value.
    ///
    /// Due to minor optimisation, it returns slightly different value than
    /// `self.from_rgb(l, l, l)` would.  Specifically, luma is in addition
    /// copied the unused byte of the colour.
    ///
    /// ```
    /// let shifts = setroot::img::RgbShifts { r: 16, g: 8, b: 0 }
    /// assert_eq!(0x42_42_42_42, shifts.from_luma(42));
    /// ```
    pub fn from_luma<S: Subpixel>(&self, luma: S) -> u32 {
        u32::from(luma.to_u8()) * 0x0101_0101
    }
}


/// A type of a single colour component.
pub trait Subpixel: bytemuck::Pod {
    /// Converts the component value into one in 0–255 range.
    fn to_u8(self) -> u8;
}

impl Subpixel for u8 {
    fn to_u8(self) -> u8 { self }
}
impl Subpixel for u16 {
    fn to_u8(self) -> u8 { (self >> 8) as u8 }
}
impl Subpixel for f32 {
    fn to_u8(self) -> u8 {
        // Handle NaNs.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(self < 1.0) {
            255
        } else if self <= 0.0 {
            0
        } else {
            (self * 255.0).round() as u8
        }
    }
}


/// A view of an 24-bit sRGB image backed by a continuous buffer of `[red,
/// green, blue]` elements.
pub trait View {
    /// Returns dimensions of the image.
    ///
    /// Note that while the return type is a tuple of 32-bit integers, in
    /// practice each dimension is limited to the range of 16-bit unsigned
    /// integers.  This discrepancy is caused by desire for greater
    /// interoperability with other crates (and `image` crate in particular)
    /// which support larger images.
    fn dimensions(&self) -> (u32, u32);

    /// Returns underlying buffer of the image.
    ///
    /// The length of the returned slice must match the dimensions.
    /// Each pixel is represented as `[red, green, blue]` 3-element arrays.
    ///
    /// Hint: `&[u8]` slice can be converted into `&[[u8; 3]]` slice using
    /// [`bytemuck::try_cast_slice`].
    fn as_rgb(&self) -> &[[u8; 3]];
}

impl<T: View> View for &T {
    fn dimensions(&self) -> (u32, u32) { (*self).dimensions() }
    fn as_rgb(&self) -> &[[u8; 3]] { (*self).as_rgb() }
}

/// An image in sRGB colour space.
pub struct Ref<'a> {
    /// Width of the image.
    ///
    /// For greater interoperability with other crates (namely `image`), the
    /// value is a 32-bit unsigned integer.  However, in practice, the width
    /// must fit a 16-bit unsigned integer or else [`RootPixmap::put_image`]
    /// will return [`Error::ImageTooLarge`].
    pub width: u32,

    /// Height of the image.
    ///
    /// For greater interoperability with other crates (namely `image`), the
    /// value is a 32-bit unsigned integer.  However, in practice, the height
    /// must fit a 16-bit unsigned integer or else [`RootPixmap::put_image`]
    /// will return [`Error::ImageTooLarge`].
    pub height: u32,

    /// The underlying image buffer of sRGB values.
    ///
    /// If the size doesn’t match dimensions [`RootPixmap::put_image`] will
    /// fail.
    pub data: &'a [[u8; 3]],
}

impl<'a> Ref<'a> {
    /// Constructs new image view for image with given dimensions and underlying
    /// raw buffer of sRGB values.
    ///
    /// Returns `None` if the size of the `data` slice does not match the
    /// dimensions, i.e. if `width * height * 3 != data.len()`.
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Option<Self> {
        let dim = width.try_into().ok().zip(height.try_into().ok());
        dim.and_then(|(w, h): (usize, usize)| w.checked_mul(h))
            .and_then(|area| area.checked_mul(3))
            .is_some_and(|len| len == data.len())
            .then(|| Self { width, height, data: bytemuck::cast_slice(data) })
    }
}

impl<'a> View for Ref<'a> {
    #[inline]
    fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
    #[inline]
    fn as_rgb(&self) -> &'a [[u8; 3]] { self.data }
}

#[cfg(feature = "image")]
impl View for image::RgbImage {
    #[inline]
    fn dimensions(&self) -> (u32, u32) { self.dimensions() }
    #[inline]
    fn as_rgb(&self) -> &[[u8; 3]] {
        bytemuck::cast_slice(self.as_raw().as_slice())
    }
}
