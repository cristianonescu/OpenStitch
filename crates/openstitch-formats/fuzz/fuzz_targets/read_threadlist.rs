#![no_main]

use libfuzzer_sys::fuzz_target;
use openstitch_formats::threadlist::ListType;

// Any input must produce Ok or Err from every thread list reader, never a panic or hang.
fuzz_target!(|data: &[u8]| {
    for ty in ListType::ALL {
        let _ = ty.read(data);
    }
});
