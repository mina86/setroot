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

pub mod err;
pub mod img;

pub use err::Error;
pub type Result<T = (), E = Error> = core::result::Result<T, E>;

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
    pub fn default_screen(&self) -> Result<&x::Screen, err::BadScreenNumber> {
        self.conn
            .get_setup()
            .roots()
            .nth(self.screen_num as usize)
            .ok_or(err::BadScreenNumber(self.screen_num))
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub name: Option<String>,
    pub primary: bool,
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
    rgb_shifts: img::RgbShifts,
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
    pub fn new(conn: &'a xcb::Connection, scr: &'a x::Screen) -> Result<Self> {
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
    fn get_rgb_shifts(scr: &'a x::Screen) -> Result<img::RgbShifts> {
        fn get_shift(mask: u32) -> Option<u8> {
            let shift = mask.trailing_zeros();
            ((mask >> shift) == 0xff).then_some(shift as u8)
        }

        let root_depth = scr.root_depth();
        let root_visual = scr.root_visual();
        scr.allowed_depths()
            .filter(|depth| depth.depth() == root_depth)
            .flat_map(|depth| depth.visuals())
            .find(|vis| vis.visual_id() == root_visual)
            .ok_or(Error::CouldNotFindRootVisual(root_visual))
            .and_then(|vis| {
                if vis.class() == x::VisualClass::TrueColor &&
                    (root_depth == 24 || root_depth == 32)
                {
                    let shifts = get_shift(vis.red_mask())
                        .zip(get_shift(vis.green_mask()))
                        .zip(get_shift(vis.blue_mask()));
                    if let Some(((r, g), b)) = shifts {
                        return Ok(img::RgbShifts { r, g, b });
                    }
                }
                Err(Error::UnsupportedVisual(root_depth, vis.class()))
            })
    }

    /// Returns RGB shifts which define pixel format used by the X display.
    ///
    /// The shifts allow converting red, green and blue components into `u32`
    /// value that X server expects.
    pub fn rgb_shifts(&self) -> img::RgbShifts { self.rgb_shifts }

    /// Puts an image at given location on the pixmap.
    ///
    /// The image must be in 24-bit sRGB (that is, 8 bits per subpixel).
    ///
    /// Returns an error if the dimensions of the image are too large (or on
    /// protocol error).
    pub fn put_image<'b>(
        &self,
        dst_x: i16,
        dst_y: i16,
        img: impl img::IntoXBuffer<'b>,
    ) -> Result {
        let (width, height) = img.dimensions()?;
        let buffer = img.into_x_buffer(self.rgb_shifts)?;
        let buffer = buffer.as_ref();
        if usize::from(width) * usize::from(height) * 4 == buffer.len() {
            self.put_raw_impl(dst_x, dst_y, width, height, buffer)
        } else {
            Err(Error::BadBufferSize(buffer.len(), width, height))
        }
    }

    /// Puts an image at given location on the pixmap.
    ///
    /// The image must be in format accepted by the X display server.  This
    /// format is described by the [`img::RgbShifts`] object returned by
    /// [`Self::rgb_shifts`] class.
    ///
    /// Usually, [`Self::put_image`] method is more convenient interface since
    /// it performs all necessary data conversion to generate format acceptable
    /// by the display server.
    #[inline]
    pub fn put_raw(
        &self,
        dst_x: i16,
        dst_y: i16,
        width: u16,
        height: u16,
        data: &[u32],
    ) -> Result {
        if usize::from(width) * usize::from(height) == data.len() {
            let data = bytemuck::must_cast_slice(data);
            self.put_raw_impl(dst_x, dst_y, width, height, data)
        } else {
            Err(Error::BadBufferSize(data.len() * 4, width, height))
        }
    }

    fn put_raw_impl(
        &self,
        dst_x: i16,
        dst_y: i16,
        width: u16,
        height: u16,
        data: &[u8],
    ) -> Result {
        self.conn
            .send_and_check_request(&x::PutImage {
                format: x::ImageFormat::ZPixmap,
                drawable: x::Drawable::Pixmap(self.pixmap),
                gc: self.gc,
                width,
                height,
                dst_x,
                dst_y,
                left_pad: 0,
                depth: self.screen.root_depth(),
                data,
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
    pub fn set_background(&self) -> Result {
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
