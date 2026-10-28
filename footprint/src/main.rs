//! Smallest program that keeps `Sentinel::<64>::observe`: linked with
//! `--entry=sentinel64_observe`, everything not reachable from it is dropped.
#![no_std]
#![no_main]

use core::mem::size_of;
use nostd_sentinel::Sentinel;

// RAM: computed by the compiler for this target, checked at build time.
const _: () = assert!(size_of::<Sentinel<64>>() == 312);

#[no_mangle]
pub extern "C" fn sentinel64_observe(s: &mut Sentinel<64>, x: i32) -> u8 {
    s.observe(x) as u8
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
