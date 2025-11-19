# setroot

A library for setting desktop background image.  It provides methods and types
for a) querying screen’s monitor configuration, b) creating a pixmap which can
be used as a background image and c) changing the root background.

The library serves the purpose similar to applications such as `Esetroot`,
`xsetroot`, `fvwm-root` etc.  Since it communicates directly with the X display
it does not require external applications to accomplish its task.


## Example usage

```rust,no_run
fn set_background() -> Result<(), setroot::Error> {
    // Connect to X display server.
    let display = setroot::Display::open()?;
    // Create a pixmap to use as the background.
    let root_pixmap = display.root_pixmap()?;
    // Query monitor configuration of the screen.
    let monitors = display.monitors()?;
    // Iterate over all monitors and draw background for each
    // of them.
    for monitor in monitors {
        // Load an image to fit the monitor.
        let image: image::RgbImage =
            load_image_for_dimension(
                monitor.width, monitor.height)?;
        // Draw the image onto the pixmap.
        root_pixmap.put_image(monitor.x, monitor.y, image)?;
    }
    // Set the background.
    root_pixmap.set_background()?;
    Ok(())
}

fn load_image_for_dimension(
    width: u16,
    height: u16,
) -> Result<image::RgbImage, setroot::Error> {
    todo!()
}
```


## Features

The crate defines the following Cargo feature:

* `image`, enabled by default, includes dependency on `image` crate and allows
  `image::RgbImage` objects to be passed as arguments to `put_image` function.


## Limitations

The library has currently tho following limitations:

* It does not offer features for resizing images or tiling images.  It is left
  to the user to prepare correctly sized image that can be draw on the root
  pixmap.

* It works with X11 display servers only and requires RandR 1.5+ extension to be
  present.  This covers vast majority of X11 displays but might not work on
  Wayland desktops or in non-Unix-like environments.

* It assumes the X display server uses 24/32-bit True Colour visual, i.e. that
  colours are represented as 32-bit numbers with 8 bits per channel.  This
  should cover *vast* majority of cases and system configurations.
