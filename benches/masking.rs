#![feature(int_to_from_bytes)]
#[macro_use]
extern crate criterion;
extern crate rand;
extern crate faster;

use criterion::Criterion;

use std::cmp::min;
use std::mem::uninitialized;
use std::ptr::{copy_nonoverlapping, read_unaligned};

use faster::*;

// BASED ON TUNGSTENITE CODE file mask.rs

/// Generate a random frame mask.
#[inline]
pub fn generate_mask() -> [u8; 4] {
    rand::random()
}

/// Mask/unmask a frame.
#[inline]
pub fn apply_mask(buf: &mut [u8], mask: [u8; 4]) {
    apply_mask_simd(buf, mask)
}

/// A safe unoptimized mask application.
#[inline]
#[allow(dead_code)]
fn apply_mask_fallback(buf: &mut [u8], mask: [u8; 4]) {
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte ^= mask[i & 3];
    }
}

/// Faster version of `apply_mask()` which operates on 4-byte blocks.
#[inline]
#[allow(dead_code)]
fn apply_mask_fast32(buf: &mut [u8], mask: [u8; 4]) {
    let mask_u32: u32 = unsafe {
        read_unaligned(mask.as_ptr() as *const u32)
    };

    let mut ptr = buf.as_mut_ptr();
    let mut len = buf.len();

    // Possible first unaligned block.
    let head = min(len, (4 - (ptr as usize & 3)) & 3);
    let mask_u32 = if head > 0 {
        unsafe {
            xor_mem(ptr, mask_u32, head);
            ptr = ptr.offset(head as isize);
        }
        len -= head;
        if cfg!(target_endian = "big") {
            mask_u32.rotate_left(8 * head as u32)
        } else {
            mask_u32.rotate_right(8 * head as u32)
        }
    } else {
        mask_u32
    };

    if len > 0 {
        debug_assert_eq!(ptr as usize % 4, 0);
    }

    // Properly aligned middle of the data.
    while len > 4 {
        unsafe {
            *(ptr as *mut u32) ^= mask_u32;
            ptr = ptr.offset(4);
            len -= 4;
        }
    }

    // Possible last block.
    if len > 0 {
        unsafe { xor_mem(ptr, mask_u32, len); }
    }
}

#[inline]
// TODO: copy_nonoverlapping here compiles to call memcpy. While it is not so inefficient,
// it could be done better. The compiler does not see that len is limited to 3.
unsafe fn xor_mem(ptr: *mut u8, mask: u32, len: usize) {
    let mut b: u32 = uninitialized();
    #[allow(trivial_casts)]
    copy_nonoverlapping(ptr, &mut b as *mut _ as *mut u8, len);
    b ^= mask;
    #[allow(trivial_casts)]
    copy_nonoverlapping(&b as *const _ as *const u8, ptr, len);
}

#[inline]
fn apply_mask_simd(buf: &mut [u8], mask: [u8; 4]) {
    // documentation on this nightly-only feature is confusing,
    // but the resulting integer seems to be big-endian
    let mask_u32 = u32::from_bytes(mask);

    let pattern = faster::u32s(mask_u32).be_u8s();
    
    buf.simd_iter(u8s(0)).simd_map(|v| v ^ pattern).scalar_collect();
}

fn masking_benchmark(c: &mut Criterion) {
    let mask = generate_mask();
    let mut data_to_mask: Vec<u8> = vec![0;10_000];
    c.bench_function("apply_mask", move |b| b.iter(|| apply_mask(&mut data_to_mask, mask)));
}

criterion_group!(benches, masking_benchmark);
criterion_main!(benches);