use crate::sys::pal::abi;
use crate::sync::atomic::{AtomicU32, Ordering};

static STATE: AtomicU32 = AtomicU32::new(0x7F4A_7C15);

fn seed() -> u64 {
    let mut real = abi::Timespec::default();
    let mut mono = abi::Timespec::default();
    let _ = unsafe { abi::clock_gettime(abi::CLOCK_REALTIME, &mut real) };
    let _ = unsafe { abi::clock_gettime(abi::CLOCK_MONOTONIC, &mut mono) };
    let stack = core::ptr::from_ref(&real).addr() as u64;
    let mixed = (real.tv_sec as u32 as u64)
        ^ ((real.tv_nsec as u32 as u64) << 17)
        ^ ((mono.tv_sec as u32 as u64) << 31)
        ^ ((mono.tv_nsec as u32 as u64) << 7)
        ^ stack.rotate_left(23);
    if mixed == 0 { 0xA076_1D64_78BD_642F } else { mixed }
}

fn fallback_fill(bytes: &mut [u8]) {
    let mixed = seed();
    let mut state = STATE.fetch_xor((mixed ^ (mixed >> 32)) as u32, Ordering::Relaxed)
        ^ mixed as u32;
    if state == 0 {
        state = 0xA0B4_28DB;
    }
    for chunk in bytes.chunks_mut(4) {
        // xorshift32; fallback only. This is not a cryptographic RNG.
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let value = state.to_ne_bytes();
        chunk.copy_from_slice(&value[..chunk.len()]);
    }
    STATE.store(state, Ordering::Relaxed);
}

fn try_system_fill(mut bytes: &mut [u8]) -> bool {
    while !bytes.is_empty() {
        let result = unsafe { abi::getrandom(bytes.as_mut_ptr(), bytes.len(), 0) };
        if result <= 0 {
            return false;
        }
        let count = (result as usize).min(bytes.len());
        bytes = &mut bytes[count..];
    }
    true
}

pub fn fill_bytes(bytes: &mut [u8]) {
    assert!(try_system_fill(bytes), "PopugOS getrandom syscall is unavailable");
}

pub fn hashmap_random_keys() -> (u64, u64) {
    let mut bytes = [0u8; 16];
    if !try_system_fill(&mut bytes) {
        fallback_fill(&mut bytes);
    }
    (
        u64::from_ne_bytes(bytes[..8].try_into().unwrap()),
        u64::from_ne_bytes(bytes[8..].try_into().unwrap()),
    )
}
