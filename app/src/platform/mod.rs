//! The macOS side: what Rune doesn't draw or do itself. NSWorkspace, user defaults, the
//! login item, the app bundle, SF Symbols rasterized for Rune, and the menu bar items.

pub mod ring;
pub mod statusbar;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageRep, NSColor, NSCompositingOperation, NSDeviceRGBColorSpace, NSFontWeightMedium,
    NSGraphicsContext, NSImage, NSImageSymbolConfiguration, NSRectFillUsingOperation, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSBundle, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL,
    NSUserDefaults,
};

// MARK: NSWorkspace

/// Opens a URL, file or app with its default handler. Paths start with `/` or `~`.
pub fn open(target: &str) {
    let Some(url) = url_for(target) else {
        eprintln!("Cantrip: can't open {target:?}");
        return;
    };
    if !NSWorkspace::sharedWorkspace().openURL(&url) {
        eprintln!("Cantrip: nothing opened {target:?}");
    }
}

pub fn reveal(path: &str) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    NSWorkspace::sharedWorkspace()
        .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
}

/// Whether the user asked for less motion (Accessibility ▸ Display ▸ Reduce motion).
pub fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

fn url_for(target: &str) -> Option<Retained<NSURL>> {
    if target.starts_with('/') || target.starts_with('~') {
        let path = crate::config::expand_tilde(target);
        return Some(NSURL::fileURLWithPath(&NSString::from_str(
            &path.to_string_lossy(),
        )));
    }
    NSURL::URLWithString(&NSString::from_str(target))
}

/// Finder's name for a file ("Safari", localized), when it has one.
pub fn display_name(path: &Path) -> Option<String> {
    use objc2::msg_send;
    let manager = objc2::runtime::AnyClass::get(c"NSFileManager")?;
    autoreleasepool(|_| unsafe {
        let manager: *mut AnyObject = msg_send![manager, defaultManager];
        let name: *mut NSString =
            msg_send![manager, displayNameAtPath: &*NSString::from_str(&path.to_string_lossy())];
        name.as_ref().map(|n| n.to_string())
    })
}

// MARK: The bundle

/// True when running from Cantrip.app, not a bare `cargo run` binary.
pub fn in_app_bundle() -> bool {
    NSBundle::mainBundle()
        .bundlePath()
        .to_string()
        .ends_with(".app")
}

pub fn bundle_resources() -> Option<PathBuf> {
    in_app_bundle()
        .then(|| NSBundle::mainBundle().resourcePath())
        .flatten()
        .map(|p| PathBuf::from(p.to_string()))
}

// MARK: User defaults (shared with the Swift app's, by bundle id)

pub fn load_usage() -> HashMap<String, i64> {
    let defaults = NSUserDefaults::standardUserDefaults();
    let Some(dict) = defaults.dictionaryForKey(&NSString::from_str("usage")) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for key in dict.allKeys() {
        let Ok(key) = key.downcast::<NSString>() else {
            continue;
        };
        let Some(value) = dict.objectForKey(&key) else {
            continue;
        };
        if let Ok(n) = value.downcast::<NSNumber>() {
            out.insert(key.to_string(), n.integerValue() as i64);
        }
    }
    out
}

pub fn save_usage(usage: &HashMap<String, i64>) {
    let keys: Vec<Retained<NSString>> = usage.keys().map(|k| NSString::from_str(k)).collect();
    let values: Vec<Retained<AnyObject>> = usage
        .values()
        .map(|v| {
            Retained::into_super(Retained::into_super(Retained::into_super(
                NSNumber::new_isize(*v as isize),
            )))
        })
        .collect();
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let dict = NSDictionary::from_retained_objects(&key_refs, &values);
    unsafe {
        NSUserDefaults::standardUserDefaults()
            .setObject_forKey(Some(&dict), &NSString::from_str("usage"))
    };
}

fn default_bool(key: &str) -> bool {
    NSUserDefaults::standardUserDefaults().boolForKey(&NSString::from_str(key))
}

fn set_default_bool(key: &str, value: bool) {
    NSUserDefaults::standardUserDefaults().setBool_forKey(value, &NSString::from_str(key));
}

// MARK: Launch at Login

pub mod login {
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    pub fn enabled() -> bool {
        unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
    }

    pub fn toggle() {
        let service = unsafe { SMAppService::mainAppService() };
        let result = unsafe {
            if enabled() {
                service.unregisterAndReturnError()
            } else {
                service.registerAndReturnError()
            }
        };
        if let Err(e) = result {
            eprintln!("Cantrip: login item change failed: {e:?}");
        }
    }

    /// A launcher should be there after a reboot, so the installed app turns this on once.
    /// Skipped for `cargo run` builds, which aren't in an app bundle.
    pub fn enable_on_first_run() {
        const KEY: &str = "didSetUpLoginItem";
        if !super::in_app_bundle() || super::default_bool(KEY) {
            return;
        }
        super::set_default_bool(KEY, true);
        if let Err(e) = unsafe { SMAppService::mainAppService().registerAndReturnError() } {
            eprintln!("Cantrip: could not register login item: {e:?}");
        }
    }
}

// MARK: Drawing for Rune

/// Draws into a `w`×`h` point bitmap at `scale` pixels per point and returns straight-alpha
/// sRGB RGBA, as `rune::Pixels::set` takes it. `draw` gets the rect in points.
pub fn rasterize(
    w: f64,
    h: f64,
    scale: f64,
    draw: impl FnOnce(NSRect),
) -> Option<(u32, u32, Vec<u8>)> {
    let (pw, ph) = (
        (w * scale).ceil().max(1.0) as isize,
        (h * scale).ceil().max(1.0) as isize,
    );
    autoreleasepool(|_| unsafe {
        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(), std::ptr::null_mut(), pw, ph, 8, 4, true, false, NSDeviceRGBColorSpace, pw * 4, 32,
        )?;
        rep.setSize(NSSize::new(w, h));
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        draw(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h)));
        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        let premultiplied = std::slice::from_raw_parts(data, (pw * ph * 4) as usize);
        Some((pw as u32, ph as u32, unpremultiply(premultiplied)))
    })
}

fn unpremultiply(px: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len());
    for p in px.as_chunks::<4>().0 {
        let a = p[3] as u32;
        let straight = |c: u8| {
            (c as u32 * 255 + a / 2)
                .checked_div(a)
                .map_or(0, |v| v.min(255) as u8)
        };
        out.extend_from_slice(&[straight(p[0]), straight(p[1]), straight(p[2]), p[3]]);
    }
    out
}

/// An SF Symbol at `point_size`, tinted `rgba`, in a `box_pt` square. `None` when the name
/// isn't a symbol on this system.
pub fn symbol_rgba(
    name: &str,
    point_size: f64,
    box_pt: f64,
    scale: f64,
    rgba: [f32; 4],
) -> Option<(u32, u32, Vec<u8>)> {
    let image = symbol_image(name, point_size)?;
    let size = image.size();
    let color = NSColor::colorWithSRGBRed_green_blue_alpha(
        rgba[0] as f64,
        rgba[1] as f64,
        rgba[2] as f64,
        rgba[3] as f64,
    );
    rasterize(box_pt, box_pt, scale, |rect| {
        // Fit inside the box, keeping the symbol's aspect, centred.
        let fit = (box_pt / size.width.max(size.height)).min(1.0);
        let (w, h) = (size.width * fit, size.height * fit);
        let at = NSRect::new(
            NSPoint::new((rect.size.width - w) / 2.0, (rect.size.height - h) / 2.0),
            NSSize::new(w, h),
        );
        image.drawInRect(at);
        color.set();
        NSRectFillUsingOperation(rect, NSCompositingOperation::SourceAtop);
    })
}

pub fn symbol_image(name: &str, point_size: f64) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )?;
    let config =
        NSImageSymbolConfiguration::configurationWithPointSize_weight(point_size, unsafe {
            NSFontWeightMedium
        });
    image.imageWithSymbolConfiguration(&config)
}
