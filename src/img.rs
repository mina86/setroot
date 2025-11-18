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
