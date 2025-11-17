// setroot — a library for setting desktop background image.
// © 2025 by Michał Nazarewicz <mina86@mina86.com>
//
// setroot is free software: you can redistribute it and/or modify it under the
// terms of the GNU Lesser General Public License as published by the Free
// Software Foundation; either version 3 of the License, or (at your option) any
// later version.
//
// setroot is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
// A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// setroot.  If not, see <http://www.gnu.org/licenses/>.

#![doc = include_str!("../README.md")]

use xcb::x::Atom;
use xcb::{Xid, XidNew, randr, x};

pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Handler for an X11 connection.
pub struct Display {
    conn: xcb::Connection,
    screen_num: i32,
}

impl Display {
    /// Opens connection to X11 display.
    pub fn open() -> Result<Self> {
        let (conn, screen_num) = xcb::Connection::connect(None)?;
        Self::from_xcb(conn, screen_num)
    }

    /// Constructs the object from existing XCB connection.
    pub fn from_xcb(conn: xcb::Connection, screen_num: i32) -> Result<Self> {
        usize::try_from(screen_num)
            .map(|_| Self { conn, screen_num })
            .map_err(|_| Error::BadScreenNumber(screen_num))
    }

    /// Consumes the object and returns the underlying XCB connection.  Returns
    /// a `(conn, screen_num)` pair just as [`xcb::Connection::connect`].
    pub fn into_xcb(self) -> (xcb::Connection, i32) {
        (self.conn, self.screen_num)
    }

    /// Returns reference to the XCB connection.
    pub fn conn(&self) -> &xcb::Connection { &self.conn }
    /// Returns the default screen number.
    pub fn default_screen_num(&self) -> i32 { self.screen_num }
    /// Returns the default screen.
    pub fn default_screen(&self) -> Result<&x::Screen> {
        self.conn
            .get_setup()
            .roots()
            .nth(self.screen_num as usize)
            .ok_or(Error::BadScreenNumber(self.screen_num))
    }

    /// Returns list of active monitors.
    ///
    /// Uses RandR extensions to query the dimensions of the monitors.  Requires
    /// RandR extension version 1.5 or newer to work.
    pub fn monitors(&self) -> Result<Vec<Monitor>> {
        let cookie = self.conn.send_request(&randr::GetMonitors {
            window: self.default_screen()?.root(),
            get_active: true,
        });
        Ok(self
            .conn
            .wait_for_reply(cookie)?
            .monitors()
            .map(|mon| Monitor {
                name: self.get_atom_name(mon.name()),
                primary: mon.primary(),
                automatic: mon.automatic(),
                x: mon.x(),
                y: mon.y(),
                width: mon.width(),
                height: mon.height(),
                width_in_millimeters: mon.width_in_millimeters(),
                height_in_millimeters: mon.height_in_millimeters(),
            })
            .collect())
    }

    /// Returns a representation of the root window’s background pixmap.
    ///
    /// The object can be used to draw on the pixmap before finally setting it
    /// as the root window’s pixmap.  It’s dimensions and depth matches the
    /// default screen’s.
    pub fn root_pixmap(&self) -> Result<RootPixmap<'_>> {
        RootPixmap::new(self.conn(), self.default_screen()?)
    }

    /// Returns the name of given atom.
    fn get_atom_name(&self, atom: Atom) -> Option<String> {
        let cookie = self.conn.send_request(&x::GetAtomName { atom });
        let reply = self.conn.wait_for_reply(cookie).ok()?;
        let name = reply.name();
        (name.len() != 0).then(|| name.to_utf8().into_owned())
    }
}

/// Description of a monitor.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Monitor {
    pub name: Option<String>,
    pub primary: bool,
    pub automatic: bool,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub width_in_millimeters: u32,
    pub height_in_millimeters: u32,
}

/// A pixmap on a root window scaled to cover the entire screen.  Used to put
/// images onto it and eventually set as wallpaper.
pub struct RootPixmap<'a> {
    conn: &'a xcb::Connection,
    screen: &'a x::Screen,
    pixmap: x::Pixmap,
    gc: x::Gcontext,
    rgb_shifts: (u8, u8, u8),
}

impl core::ops::Drop for RootPixmap<'_> {
    fn drop(&mut self) {
        self.conn.send_request(&x::FreeGc { gc: self.gc });
        self.conn.send_request(&x::FreePixmap { pixmap: self.pixmap });
    }
}

impl<'a> RootPixmap<'a> {
    /// Constructs a new pixmap tied to the screen’s root window and sized to
    /// match screen’s dimensions.
    fn new(conn: &'a xcb::Connection, scr: &'a x::Screen) -> Result<Self> {
        // Verify the visual and get R, G and B shifts for later use.
        let rgb_shifts = Self::get_rgb_shifts(scr)?;

        let pixmap = conn.generate_id::<x::Pixmap>();
        conn.send_and_check_request(&x::CreatePixmap {
            depth: scr.root_depth(),
            pid: pixmap,
            drawable: x::Drawable::Window(scr.root()),
            width: scr.width_in_pixels(),
            height: scr.height_in_pixels(),
        })?;

        let gc = conn.generate_id::<x::Gcontext>();
        let cookie = conn.send_request_checked(&x::CreateGc {
            cid: gc,
            drawable: x::Drawable::Pixmap(pixmap),
            value_list: &[],
        });
        conn.check_request(cookie).inspect_err(|_| {
            conn.send_request(&x::FreePixmap { pixmap });
        })?;

        Ok(Self { conn, screen: scr, pixmap, gc, rgb_shifts })
    }

    /// Checks that visual is one we support and returns R, G and B channel
    /// sifts.
    fn get_rgb_shifts(scr: &'a x::Screen) -> Result<(u8, u8, u8)> {
        let scr_depth = scr.root_depth();
        let visual = scr
            .allowed_depths()
            .filter(|depth| depth.depth() == scr_depth)
            .flat_map(|depth| depth.visuals())
            .find(|vis| vis.visual_id() == scr.root_visual())
            .ok_or(Error::CouldNotFindRootVisual(scr.root_visual()))?;

        // We only support 24 or 32-bit TrueColor
        if visual.class() != x::VisualClass::TrueColor ||
            (scr.root_depth() != 24 && scr.root_depth() != 32)
        {
            return Err(Error::UnsupportedVisual(scr_depth, visual.class()));
        }

        // Get the color channel shifts from the visual's masks
        let r_shift = visual.red_mask().trailing_zeros() as u8;
        let g_shift = visual.green_mask().trailing_zeros() as u8;
        let b_shift = visual.blue_mask().trailing_zeros() as u8;
        Ok((r_shift, g_shift, b_shift))
    }

    /// Puts an image at given location on the pixmap.
    ///
    /// The image must be in 24-bit sRGB (that is, 8 bits per subpixel).
    ///
    /// Returns an error if the dimensions of the image are too large (or on
    /// protocol error).
    pub fn put_image(&self, x: i16, y: i16, img: impl ImageView) -> Result<()> {
        let (img_width, img_height) = img.dimensions();
        let dim = img_width.try_into().ok().zip(img_height.try_into().ok());
        let (img_width, img_height): (u16, u16) =
            dim.ok_or(Error::ImageTooLarge(img_width, img_height))?;

        let img = img.as_rgb();
        if usize::from(img_width) * usize::from(img_height) == img.len() {
            self.put_image_impl(x, y, img_width, img_height, img)
        } else {
            Err(Error::BadImageBufferSize(img_width, img_height, img.len()))
        }
    }

    fn put_image_impl(
        &self,
        x: i16,
        y: i16,
        img_width: u16,
        img_height: u16,
        img: &[[u8; 3]],
    ) -> Result<()> {
        // Convert 24-bit RGB to 32-bit (X)RGB buffer.
        let (r_shift, g_shift, b_shift) = self.rgb_shifts;
        let data = img
            .iter()
            .map(|&[r, g, b]| {
                (u32::from(r) << r_shift) |
                    (u32::from(g) << g_shift) |
                    (u32::from(b) << b_shift)
            })
            .collect::<Vec<u32>>();

        // Send the PutImage request.
        self.conn
            .send_and_check_request(&x::PutImage {
                format: x::ImageFormat::ZPixmap,
                drawable: x::Drawable::Pixmap(self.pixmap),
                gc: self.gc,
                width: img_width,
                height: img_height,
                dst_x: x,
                dst_y: y,
                left_pad: 0,
                depth: self.screen.root_depth(),
                data: bytemuck::cast_slice(data.as_slice()),
            })
            .map_err(Error::from)
    }

    /// Set the root pixmap as the background of the root window.
    ///
    /// Furthermore, updates `_XROOTPMAP_ID` and `ESETROOT_PMAP_ID` atoms.
    ///
    /// If the method returns an error, the state of the root window is
    /// unspecified.  For example, it’s possible that the atoms were updated
    /// (such that applications using pseudo translucency will use the new
    /// background) but the actual background of the root window has not been
    /// updated.
    ///
    /// Furthermore, the method ignores some errors so long as the back pixmap
    /// of the root window is set.  This may result in redrawing artefacts.
    pub fn set_background(&self) -> Result<()> {
        self.set_root_atoms();

        self.conn.send_request(&x::KillClient {
            resource: 0, // AllTemporary
        });
        self.conn.send_request(&x::SetCloseDownMode {
            mode: x::CloseDown::RetainTemporary,
        });

        self.conn.send_and_check_request(&x::ChangeWindowAttributes {
            window: self.screen.root(),
            value_list: &[x::Cw::BackPixmap(self.pixmap)],
        })?;
        self.conn.send_request(&x::ClearArea {
            exposures: false,
            window: self.screen.root(),
            x: 0,
            y: 0,
            width: self.screen.width_in_pixels(),
            height: self.screen.height_in_pixels(),
        });
        Ok(())
    }

    /// Updates the atoms holding the root pixmap.
    fn set_root_atoms(&self) {
        let mut killed = None;
        for name in ["_XROOTPMAP_ID", "ESETROOT_PMAP_ID"] {
            // Intern the atom
            let mut intern_request =
                x::InternAtom { only_if_exists: true, name: name.as_bytes() };
            let cookie = self.conn.send_request(&intern_request);
            let atom = self.conn.wait_for_reply(cookie).and_then(|reply| {
                let atom = reply.atom();
                if atom.is_none() {
                    // Atom doesn't exist, create it
                    intern_request.only_if_exists = false;
                    let cookie = self.conn.send_request(&intern_request);
                    self.conn.wait_for_reply(cookie).map(|reply| reply.atom())
                } else {
                    // Atom exists, clean up old pixmap
                    self.clean_root_atom(atom, &mut killed);
                    Ok(atom)
                }
            });
            // TODO(mpn): Report the errors.
            let atom = match atom {
                Err(_err) => {
                    //err!("x: InternAtom({}): {}", name, err);
                    return;
                }
                Ok(atom) if atom.is_none() => {
                    //err!("x: failed to create {} atom", name);
                    return;
                }
                Ok(atom) => atom,
            };

            // Change Property
            self.conn.send_request(&x::ChangeProperty {
                mode: x::PropMode::Replace,
                window: self.screen.root(),
                property: atom,
                r#type: x::ATOM_PIXMAP,
                data: &[self.pixmap.resource_id()],
            });
            // TODO(mpn): Report the error.
            // if let Err(err) = self.conn.check_request(cookie) {
            //     err!("x: ChangeProperty({}): {}", name, err);
            // }
        }
    }

    /// Cleans up old atoms holding the root pixmap.
    fn clean_root_atom(&self, atom: Atom, prev_killed: &mut Option<u32>) {
        let cookie = self.conn.send_request(&x::GetProperty {
            delete: false,
            window: self.screen.root(),
            property: atom,
            r#type: x::ATOM_ANY,
            long_offset: 0,
            long_length: 1, // We only want 1 item (the pixmap ID)
        });
        let reply = match self.conn.wait_for_reply(cookie) {
            Ok(reply) => reply,
            Err(_err) => {
                // TODO(mpn): Report the error.
                //err!("x: GetProperty({}): {}", name, err);
                return;
            }
        };

        if reply.r#type() == x::ATOM_PIXMAP &&
            reply.format() == 32 &&
            reply.length() == 1 &&
            reply.bytes_after() == 0
        {
            let pixmap_id =
                reply.value::<u32>().first().copied().unwrap_or_default();
            if pixmap_id != 0 && Some(pixmap_id) != *prev_killed {
                let pixmap = unsafe { x::Pixmap::new(pixmap_id) };
                self.conn.send_request(&x::KillClient {
                    resource: pixmap.resource_id(),
                });
                *prev_killed = Some(pixmap_id);
            }
        }
    }
}

/// A view of an 24-bit sRGB image backed by a continuous buffer of `[red,
/// green, blue]` elements.
pub trait ImageView {
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

impl<T: ImageView> ImageView for &T {
    fn dimensions(&self) -> (u32, u32) { (*self).dimensions() }
    fn as_rgb(&self) -> &[[u8; 3]] { (*self).as_rgb() }
}

/// An image in sRGB colour space.
pub struct ImageRef<'a> {
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

impl<'a> ImageRef<'a> {
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

impl<'a> ImageView for ImageRef<'a> {
    #[inline]
    fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
    #[inline]
    fn as_rgb(&self) -> &'a [[u8; 3]] { self.data }
}

#[cfg(feature = "image")]
impl ImageView for image::RgbImage {
    #[inline]
    fn dimensions(&self) -> (u32, u32) { self.dimensions() }
    #[inline]
    fn as_rgb(&self) -> &[[u8; 3]] {
        bytemuck::cast_slice(self.as_raw().as_slice())
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// XCB error as a result of an X request.
    XcbError(xcb::Error),
    /// Unrecognised screen number, i.e. negative or does not match any existing
    /// screen.
    BadScreenNumber(i32),
    /// Display server uses unsupported visual.  This library currently supports
    /// only 24 or 32-bit TrueColour visual.
    UnsupportedVisual(u8, x::VisualClass),
    /// Failed to locate visual that matches the root visual.
    CouldNotFindRootVisual(x::Visualid),
    /// Image too large.  Image dimensions must fit 16-bit unsigned integer.
    ImageTooLarge(u32, u32),
    /// The image buffer size does not match image dimensions.
    BadImageBufferSize(u16, u16, usize),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmtr: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            Self::XcbError(err) => err.fmt(fmtr),
            Self::BadScreenNumber(num) => {
                write!(fmtr, "invalid screen number: {num}")
            }
            Self::UnsupportedVisual(depth, cls) => {
                write!(
                    fmtr,
                    "unsupported visual class: {depth}-bit {cls:?}; expected \
                     24/32-bit TrueColor"
                )
            }
            Self::CouldNotFindRootVisual(visual) => {
                write!(fmtr, "could not find root visual {visual}")
            }
            Self::ImageTooLarge(width, height) => {
                write!(fmtr, "image {width}x{height} too large")
            }
            Self::BadImageBufferSize(width, height, size) => {
                write!(
                    fmtr,
                    "bad buffer size {size}*3 for {width}x{height} image"
                )
            }
        }
    }
}

macro_rules! err_from_xcb_impl {
    ($($Err:ty),*) => {
        $(
            impl From<$Err> for Error {
                fn from(err: $Err) -> Self { Self::XcbError(err.into()) }
            }
        )*
    }
}
err_from_xcb_impl!(xcb::Error, xcb::ConnError, xcb::ProtocolError);
