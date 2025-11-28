use std::borrow::Cow;

use crate::{Error, err};

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
    /// let shifts = setroot::img::RgbShifts { r: 16, g: 8, b: 0 };
    /// assert_eq!(0x00_FF_F8_E7, shifts.from_rgb(0xFFFFu16, 0xF8F8, 0xE7E7));
    ///
    /// let colour = shifts.from_rgb(1u8, 2, 3);
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
    /// let shifts = setroot::img::RgbShifts { r: 16, g: 8, b: 0 };
    /// assert_eq!(0x42_42_42_42, shifts.from_luma(0x42u8));
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
    /// Returns the value unchanged since the component is already in 0–255
    /// range.
    ///
    /// ```
    /// use setroot::img::Subpixel;
    ///
    /// assert_eq!(255, 255u8.to_u8());
    /// assert_eq!(0, 0u8.to_u8());
    /// ```
    fn to_u8(self) -> u8 { self }
}

impl Subpixel for u16 {
    /// Returns the most significant byte of the value thus reducing it to 0–255
    /// range.
    ///
    /// ```
    /// use setroot::img::Subpixel;
    ///
    /// assert_eq!(0xFF, 0xFFFFu16.to_u8());
    /// assert_eq!(0x00, 0x00FFu16.to_u8());
    /// assert_eq!(0x12, 0x1234u16.to_u8());
    /// ```
    fn to_u8(self) -> u8 { (self >> 8) as u8 }
}

impl Subpixel for f32 {
    /// Clamps value to 0.0–1.0 scale and then scales to 0–255 integer value.
    ///
    /// Negative values and values greater than one are considered invalid thus
    /// they are clamped to the valid range boundaries.
    ///
    /// ```
    /// use setroot::img::Subpixel;
    ///
    /// assert_eq!(255, 1.0f32.to_u8());
    /// assert_eq!(0, 0.0f32.to_u8());
    /// assert_eq!(128, 0.5f32.to_u8());
    ///
    /// assert_eq!(255, 1.5f32.to_u8());
    /// assert_eq!(0, (-1.0f32).to_u8());
    /// ```
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

/// A image which can be converted into an image in format supported by
/// the X display server format.
pub trait IntoXBuffer<'a> {
    /// Container of the image buffer.
    type Buffer: AsRef<[u8]>;

    /// Returns dimensions of the image.
    ///
    /// Hint: [`new_dimensions`] function can be used to convert dimensions
    /// expressed as `u32` values into values this method can returns.
    fn dimensions(&self) -> Result<(u16, u16), err::ImageTooLarge>;

    /// Returns the image buffer in format supported by the X display server.
    ///
    /// The format is defined by the `rgb_shifts` argument.  Each pixel is
    /// represented by `u32` with red, green and blue values shifted by number
    /// of bits specified in the `rgb_shifts` tuple.
    fn into_x_buffer(
        self,
        rgb_shifts: RgbShifts,
    ) -> crate::Result<Self::Buffer>;
}

/// Converts image dimensions into `(u16, u16)` pair.  Returns an error if
/// either dimension exceeds range of `u16`.
///
/// This is a helper function for implementing [`IntoXBuffer::dimensions`]
/// method.
///
/// ```
/// # use setroot::img::new_dimensions;
/// # use setroot::err;
///
/// assert_eq!(Ok((100, 200)), new_dimensions((100u32, 200u32)));
/// assert_eq!(Err(err::ImageTooLarge(70_000, 100)),
///            new_dimensions((70_000u32, 100u32)));
/// ```
pub fn new_dimensions<T: Copy + Into<u32> + TryInto<u16>>(
    dimensions: (T, T),
) -> Result<(u16, u16), err::ImageTooLarge> {
    let (width, height) = dimensions;
    width
        .try_into()
        .ok()
        .zip(height.try_into().ok())
        .ok_or_else(|| err::ImageTooLarge(width.into(), height.into()))
}



#[derive(Clone)]
struct InnerImage<'a, S: Clone> {
    dimensions: (u16, u16),
    data: Cow<'a, [S]>,
}

impl<'a, S: Clone> InnerImage<'a, S> {
    pub fn new(
        width: u32,
        height: u32,
        data: Cow<'a, [S]>,
        channels: usize,
    ) -> Result<Self, Error> {
        let (width, height) = new_dimensions((width, height))?;
        if usize::from(width) * usize::from(height) * channels == data.len() {
            Ok(Self { dimensions: (width, height), data })
        } else {
            let len = data.len() * core::mem::size_of::<S>();
            Err(Error::BadBufferSize(len, width, height))
        }
    }
}

#[derive(Clone, derive_more::AsRef, derive_more::Deref)]
#[as_ref(Vec<u32>, [u32])]
pub struct XBuffer(Vec<u32>);

impl AsRef<[u8]> for XBuffer {
    fn as_ref(&self) -> &[u8] { bytemuck::must_cast_slice(self.0.as_slice()) }
}

// https://danielkeep.github.io/tlborm/book/blk-counting.html
macro_rules! replace_expr {
    ($_t:tt $sub:expr) => {
        $sub
    };
}
macro_rules! count_tts {
    ($($tts:tt)*) => {0usize $(+ replace_expr!($tts 1usize))*};
}

macro_rules! make_image_type {
    // TODO(mina86): I would have sworn there was a better way to match
    // docstring and other annotations.
    ($(#[doc = $doc:expr])* $Image:ident; |[$($ch:ident),*], $rgb_shifts:ident| $body:expr) => {
        $(#[doc = $doc])*
        #[derive(Clone)]
        pub struct $Image<'a, S: Clone>(InnerImage<'a, S>);

        impl<'a, S: Clone> $Image<'a, S> {
            /// Constructs a new image with given data.
            pub fn new(
                width: u32,
                height: u32,
                data: Cow<'a, [S]>,
            ) -> Result<Self, Error> {
                let channels = count_tts!($($ch)*);
                InnerImage::new(width, height, data, channels).map(Self)
            }
        }

        impl<'a, S: Subpixel> IntoXBuffer<'a> for $Image<'a, S> {
            type Buffer = XBuffer;
            fn dimensions(&self) -> Result<(u16, u16), err::ImageTooLarge> { Ok(self.0.dimensions) }
            fn into_x_buffer(self, $rgb_shifts: RgbShifts) -> crate::Result<Self::Buffer> {
                let (chunks, remainder) = self.0.data.as_chunks();
                assert_eq!(0, remainder.len());
                Ok(XBuffer(chunks.iter().map(|&[$($ch),*]| $body).collect()))
            }
        }
    }
}

make_image_type! {
    /// An image in RGB format and sRGB colour space.
    ///
    /// # Example
    ///
    /// ```
    /// # use setroot::img::{RgbImage, RgbShifts};
    /// use setroot::img::IntoXBuffer;
    ///
    /// // Construct 2×1 RGB image.
    /// let data: &[u8] = &[1, 2, 3, 4, 5, 6][..];
    /// let img = RgbImage::new(2, 1, data.into()).unwrap();
    ///
    /// assert_eq!(Ok((2, 1)), img.dimensions());
    ///
    /// // Convert to X Buffer.
    /// let shifts = RgbShifts { r: 16, g: 8, b: 0 };
    /// let xbuf = img.into_x_buffer(shifts).unwrap();
    /// let xbuf: &[u8] = xbuf.as_ref();
    ///
    /// if cfg!(target_endian = "little") {
    ///     assert_eq!(&[3, 2, 1, 0, 6, 5, 4, 0], xbuf);
    /// } else  {
    ///     assert_eq!(&[0, 1, 2, 3, 0, 4, 5, 6], xbuf);
    /// }
    /// ```
    RgbImage; |[r, g, b], rgb_shifts| rgb_shifts.from_rgb(r, g, b)
}
make_image_type! {
    /// An image in RGBA format and sRGB colour space.
    ///
    /// Alpha channel is ignored when converting to X-compatible image buffer.
    ///
    /// # Example
    ///
    /// ```
    /// # use setroot::img::{RgbaImage, RgbShifts};
    /// use setroot::img::IntoXBuffer;
    ///
    /// // Construct 1×1 RGBA image.
    /// let data: &[u8] = &[0x10, 0x20, 0x30, 255][..];
    /// let img = RgbaImage::new(1, 1, data.into()).unwrap();
    ///
    /// assert_eq!(Ok((1, 1)), img.dimensions());
    ///
    /// // Convert to X Buffer.
    /// let shifts = RgbShifts { r: 16, g: 8, b: 0 };
    /// let xbuf = img.into_x_buffer(shifts).unwrap();
    /// let xbuf: &[u8] = xbuf.as_ref();
    ///
    /// let colour = u32::from_ne_bytes(xbuf.try_into().unwrap());
    /// assert_eq!(0x00102030, colour);
    /// ```
    RgbaImage; |[r, g, b, _alpha], rgb_shifts| rgb_shifts.from_rgb(r, g, b)
}
make_image_type! {
    /// An greyscale image in sRGB colour space.
    ///
    /// # Example
    ///
    /// ```
    /// # use setroot::img::{LumaImage, RgbShifts};
    /// use setroot::img::IntoXBuffer;
    ///
    /// // Construct 2×1 greyscale image with f32 subpixels.
    /// let data: &[f32] = &[0.5, 0.75][..];
    /// let img = LumaImage::new(2, 1, data.into()).unwrap();
    ///
    /// assert_eq!(Ok((2, 1)), img.dimensions());
    ///
    /// // Convert to X Buffer.
    /// let shifts = RgbShifts { r: 16, g: 8, b: 0 };
    /// let xbuf = img.into_x_buffer(shifts).unwrap();
    /// let xbuf: &[u8] = xbuf.as_ref();
    ///
    /// assert_eq!(&[128, 128, 128, 128, 191, 191, 191, 191], xbuf);
    /// ```
    LumaImage; |[y], rgb_shifts| rgb_shifts.from_luma(y)
}
make_image_type! {
    /// An greyscale image with alpha channel in sRGB colour space.
    ///
    /// Alpha channel is ignored when converting to X-compatible image buffer.
    ///
    /// # Example
    ///
    /// ```
    /// # use setroot::img::{LumaAImage, RgbShifts};
    /// use setroot::img::IntoXBuffer;
    ///
    /// // Construct 2×1 greyscale image.
    /// let data: &[u8] = &[10, 255, 20, 255][..];
    /// let img = LumaAImage::new(2, 1, data.into()).unwrap();
    ///
    /// assert_eq!(Ok((2, 1)), img.dimensions());
    ///
    /// // Convert to X Buffer.
    /// let shifts = RgbShifts { r: 16, g: 8, b: 0 };
    /// let xbuf = img.into_x_buffer(shifts).unwrap();
    /// let xbuf: &[u8] = xbuf.as_ref();
    ///
    /// assert_eq!(&[10, 10, 10, 10, 20, 20, 20, 20], xbuf);
    /// ```
    LumaAImage; |[y, _alpha], rgb_shifts| rgb_shifts.from_luma(y)
}

#[test]
fn test_buffer_size_mismatch() {
    // 2×2 image with 4 pixels = 12 bytes
    let data: [u8; 13] = [0; 13];
    let res = RgbImage::new(2, 2, (&data[..11]).into());
    assert!(matches!(res, Err(Error::BadBufferSize(11, 2, 2))));
    let res = RgbImage::new(2, 2, (&data[..13]).into());
    assert!(matches!(res, Err(Error::BadBufferSize(13, 2, 2))));
    RgbImage::new(2, 2, (&data[..12]).into()).unwrap();
}

#[cfg(feature = "image")]
impl IntoXBuffer<'static> for image::DynamicImage {
    type Buffer = Vec<u8>;

    fn dimensions(&self) -> Result<(u16, u16), err::ImageTooLarge> {
        new_dimensions(image::GenericImageView::dimensions(self))
    }

    /// Returns the image buffer in format supported by the X display server.
    ///
    /// // Construct a 2×2 DynamicImage.
    /// let mut buffer = image::RgbImage::new(2, 2);
    /// buffer.put_pixel(0, 0, image::Rgb([255, 255, 255]));
    /// buffer.put_pixel(1, 1, image::Rgb([255, 255, 255]));
    /// buffer.put_pixel(0, 1, image::Rgb([220, 20, 60]));
    /// buffer.put_pixel(1, 0, image::Rgb([220, 20, 60]));
    /// let img = image::DynamicImage::from(buffer);
    ///
    /// assert_eq!(Ok((2, 2)), img.dimensions());
    ///
    /// // Convert to X Buffer.
    /// let shifts = RgbShifts { r: 16, g: 8, b: 0 };
    /// let xbuf = img.into_x_buffer(shifts).unwrap();
    /// let xbuf: &[u8] = xbuf.as_ref();
    /// let pixels = xbuf.as_chunks::<4>()
    ///     .0
    ///     .iter()
    ///     .map(|px| u32::from_ne_bytes(*px))
    ///     .collect::<Vec<_>>();
    ///
    /// assert_eq!([0xFFFFFF, 0xDC143C, 0xDC143C, 0xFFFFFF],
    ///            pixels.as_slice());
    /// ```
    fn into_x_buffer(
        mut self,
        rgb_shifts: RgbShifts,
    ) -> crate::Result<Self::Buffer> {
        // https://github.com/image-rs/image/discussions/2650#discussioncomment-15015355
        if let Some(rgba) = self.as_mut_rgba8() {
            rgba.apply_color_space(
                image::metadata::Cicp::SRGB,
                Default::default(),
            )?;
            Ok(fix_channel_order(self.into_rgba8().into_vec(), rgb_shifts))
        } else {
            (&self).into_x_buffer(rgb_shifts)
        }
    }
}

#[cfg(feature = "image")]
impl IntoXBuffer<'static> for &image::DynamicImage {
    type Buffer = Vec<u8>;

    fn dimensions(&self) -> Result<(u16, u16), err::ImageTooLarge> {
        new_dimensions(image::GenericImageView::dimensions(*self))
    }

    fn into_x_buffer(
        self,
        rgb_shifts: RgbShifts,
    ) -> crate::Result<Self::Buffer> {
        // https://github.com/image-rs/image/discussions/2650#discussioncomment-15015296
        let (width, height) = image::GenericImageView::dimensions(self);
        let img = image::RgbaImage::new(width, height);
        let mut img = image::DynamicImage::ImageRgba8(img);
        img.copy_from_color_space(self, Default::default())?;
        // Note: We know img is Rgb8 so this doesn’t allocate.
        Ok(fix_channel_order(img.into_rgba8().into_vec(), rgb_shifts))
    }
}

#[cfg(feature = "image")]
fn fix_channel_order(mut data: Vec<u8>, rgb_shifts: RgbShifts) -> Vec<u8> {
    if rgb_shifts.from_rgb(1u8, 2u8, 3u8).to_ne_bytes() != [1u8, 2, 3, 0] {
        let (chunks, remainder) = data.as_chunks_mut();
        assert_eq!(0, remainder.len());
        for chunk in chunks {
            let [r, g, b, _] = *chunk;
            *chunk = rgb_shifts.from_rgb(r, g, b).to_ne_bytes();
        }
    }
    data
}
