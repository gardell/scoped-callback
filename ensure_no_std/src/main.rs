#![feature(alloc_error_handler, allocator_api)]
#![no_std]
#![no_main]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

struct MyAllocator;

unsafe impl GlobalAlloc for MyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static A: MyAllocator = MyAllocator;

#[alloc_error_handler]
fn my_example_handler(layout: core::alloc::Layout) -> ! {
    panic!("memory allocation of {} bytes failed", layout.size())
}

use core::panic::PanicInfo;
#[allow(unused_imports)]
use scoped_callback;

/// This function is called on panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    panic!()
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    panic!()
}
