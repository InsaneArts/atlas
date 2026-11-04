//! Match text to the helper's current keyboard layout as well as its Unicode
//! payload. Some app frameworks retranslate physical key codes and ignore the
//! Unicode override; key code zero for every character therefore types "a".
use atlas_core::actions::SHIFT;
use std::{collections::HashMap, sync::Arc};

type Mapping = HashMap<Vec<u16>, (u16, u64)>;

#[derive(Default)]
pub(super) struct Plan(Option<Arc<Mapping>>);

impl Plan {
    /// This runs in the non-UI helper input process, never the controller.
    pub(super) fn current(keyboard_type: u32) -> Self {
        Self(system::current(keyboard_type))
    }

    pub(super) fn key(&self, units: &[u16]) -> (u16, u64) {
        self.0
            .as_ref()
            .and_then(|mapping| mapping.get(units))
            .copied()
            .unwrap_or((0, 0))
    }
}

struct Translation {
    units: Vec<u16>,
    dead_key_state: u32,
}

/// Carbon's modifier byte uses shiftKey >> 8. Option can mean Meta in a
/// terminal, so automatic text mapping must never introduce Option, Command
/// or Control chords or compose a dead-key sequence.
const MODIFIERS: [(u64, u32); 2] = [(0, 0), (SHIFT, 2)];

fn printable_key(code: u16) -> bool {
    // Text positions, including ISO/JIS and keypad characters. Exclude
    // Return/Tab, navigation, modifiers, Caps Lock and input-source switches,
    // even if a custom layout provides a glyph for one of their key codes.
    matches!(code, 0..=35 | 37..=47 | 49..=50 | 65 | 67 | 69 | 75 | 78 | 81..=89 | 91..=95)
}

fn reverse_map(mut translate: impl FnMut(u16, u32) -> Option<Translation>) -> Mapping {
    let mut mapping = Mapping::new();
    for (flags, modifiers) in MODIFIERS {
        for code in (0..128).filter(|code| printable_key(*code)) {
            let Some(translated) = translate(code, modifiers) else {
                continue;
            };
            if translated.dead_key_state != 0
                || translated.units.is_empty()
                || translated.units.len() > 255
                || char::decode_utf16(translated.units.iter().copied())
                    .any(|value| value.is_err() || value.is_ok_and(char::is_control))
            {
                continue;
            }
            mapping.entry(translated.units).or_insert((code, flags));
        }
    }
    mapping
}

struct Cached {
    bytes: Vec<u8>,
    keyboard_type: u32,
    mapping: Arc<Mapping>,
}

#[derive(Default)]
struct Cache(Option<Cached>);

impl Cache {
    fn mapping(
        &mut self,
        bytes: Vec<u8>,
        keyboard_type: u32,
        translate: impl FnMut(u16, u32) -> Option<Translation>,
    ) -> Arc<Mapping> {
        if let Some(cached) = &self.0
            && cached.keyboard_type == keyboard_type
            && cached.bytes == bytes
        {
            return cached.mapping.clone();
        }
        let mapping = Arc::new(reverse_map(translate));
        self.0 = Some(Cached {
            bytes,
            keyboard_type,
            mapping: mapping.clone(),
        });
        mapping
    }
}

/// No binding for these legacy Carbon layout functions is available in the
/// existing dependency set. Keep their ownership, ABI and threading in one
/// narrow boundary; the rest of text planning is pure Rust.
#[allow(
    unsafe_code,
    reason = "FFI: Carbon TIS and UCKeyTranslate have no existing safe binding"
)]
mod system {
    use super::{Cache, Mapping, Translation};
    use objc2_core_foundation::{CFData, CFRetained, CFString, CFType};
    use std::{
        ffi::{c_ulong, c_void},
        ptr::NonNull,
        sync::{Arc, Mutex},
    };

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut CFType;
        fn TISGetInputSourceProperty(source: *const CFType, key: *const CFString) -> *mut CFType;
        static kTISPropertyUnicodeKeyLayoutData: *const CFString;
        fn UCKeyTranslate(
            layout: *const c_void,
            code: u16,
            action: u16,
            modifiers: u32,
            keyboard_type: u32,
            options: u32,
            dead_key_state: *mut u32,
            capacity: c_ulong,
            length: *mut c_ulong,
            text: *mut u16,
        ) -> i32;
    }

    pub(super) fn current(keyboard_type: u32) -> Option<Arc<Mapping>> {
        // Apple's TextInputSources.h permits a non-UI command/launchagent to
        // call TIS off-main only when those calls are not concurrent. All TIS
        // calls, property retention and translations stay under this mutex.
        static CACHE: Mutex<Cache> = Mutex::new(Cache(None));
        let mut cache = CACHE.lock().ok()?;
        // SAFETY: Copy returns a +1 CoreFoundation input source, or null.
        let source = NonNull::new(unsafe { TISCopyCurrentKeyboardLayoutInputSource() })?;
        // SAFETY: this non-null source follows the CF Copy ownership rule.
        let source = unsafe { CFRetained::from_raw(source) };
        // SAFETY: a live source and the immutable property key exported by TIS.
        let property = NonNull::new(unsafe {
            TISGetInputSourceProperty(&*source, kTISPropertyUnicodeKeyLayoutData)
        })?;
        // SAFETY: TIS returns a borrowed CF object owned by the retained source;
        // retain it before inspecting its concrete type or releasing the source.
        let property = unsafe { CFRetained::retain(property) };
        let data = property.downcast::<CFData>().ok()?;
        // A uchr header plus its first keyboard-type entry occupies 40 bytes.
        // Refuse unexpected resources without changing the Unicode fallback.
        if data.len() < 40 || data.len() > 1024 * 1024 || data.byte_ptr().is_null() {
            return None;
        }
        let bytes = data.to_vec();
        Some(cache.mapping(bytes, keyboard_type, |code, modifiers| {
            translate(&data, keyboard_type, code, modifiers)
        }))
    }

    fn translate(
        data: &CFData,
        keyboard_type: u32,
        code: u16,
        modifiers: u32,
    ) -> Option<Translation> {
        let mut units = [0_u16; 255];
        let mut length: c_ulong = 0;
        let mut dead_key_state = 0_u32;
        // SAFETY: retained immutable uchr bytes came from TIS, not a request.
        // All output pointers address their correctly-sized live Rust values.
        // UniCharCount is unsigned long in MacTypes.h (64 bits on macOS),
        // whereas OSStatus/dead state/options are 32 bits. Each trial begins
        // with fresh dead-key state; options=0 lets us reject physical dead keys.
        let status = unsafe {
            UCKeyTranslate(
                data.byte_ptr().cast(),
                code,
                0,
                modifiers,
                keyboard_type,
                0,
                &mut dead_key_state,
                units.len() as c_ulong,
                &mut length,
                units.as_mut_ptr(),
            )
        };
        if status != 0 || length == 0 || length > units.len() as c_ulong {
            return None;
        }
        Some(Translation {
            units: units[..length as usize].to_vec(),
            dead_key_state,
        })
    }
}

#[cfg(test)]
mod tests;
