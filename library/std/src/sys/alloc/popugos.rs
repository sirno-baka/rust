use crate::alloc::Layout;
use crate::ptr;
use crate::sys::pal::abi::{self, MmapArgs};

const PROT_READ: u32 = 1;
const PROT_WRITE: u32 = 2;
const MAP_PRIVATE: u32 = 2;
const MAP_ANONYMOUS: u32 = 0x20;
const HEADER_WORDS: usize = 2;

#[inline]
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    let header = HEADER_WORDS * size_of::<usize>();
    let Some(total) = layout.size().checked_add(layout.align()).and_then(|n| n.checked_add(header))
    else {
        return ptr::null_mut();
    };
    let Ok(len) = u32::try_from(total) else {
        return ptr::null_mut();
    };
    let args = MmapArgs {
        addr: 0,
        len,
        prot: PROT_READ | PROT_WRITE,
        flags: MAP_PRIVATE | MAP_ANONYMOUS,
        fd: -1,
        offset: 0,
    };
    let base = unsafe { abi::mmap(&args) };
    if base < 0 {
        return ptr::null_mut();
    }
    let base = ptr::with_exposed_provenance_mut::<u8>(base as usize);
    let aligned = (base.addr() + header + layout.align() - 1) & !(layout.align() - 1);
    let metadata = base.with_addr(aligned - header).cast::<usize>();
    unsafe {
        metadata.write(base.addr());
        metadata.add(1).write(total);
    }
    base.with_addr(aligned)
}

#[inline]
pub unsafe fn dealloc(ptr: *mut u8, _layout: Layout) {
    if ptr.is_null() {
        return;
    }
    let metadata = unsafe { ptr.cast::<usize>().sub(HEADER_WORDS) };
    let base = ptr.with_addr(unsafe { metadata.read() });
    let len = unsafe { metadata.add(1).read() };
    let _ = unsafe { abi::munmap(base, len) };
}

#[inline]
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unsafe { super::realloc_fallback(ptr, layout, new_size) }
}

#[inline]
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unsafe { alloc(layout) }
}
