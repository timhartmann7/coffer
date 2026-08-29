//! Erasing memory the language has stopped tracking.
//!
//! A vault can wipe what it still owns. It cannot wipe what somebody else
//! allocated, copied and freed on the way: opening a KDBX file decompresses the
//! whole database into a buffer, deserialises it into a second one, decrypts
//! every protected value into a third, and frees all three. None of those
//! belong to Coffer, none of them are zeroized by the library that made them,
//! and after an auto-lock every one of them is still readable in this process.
//!
//! Measured: a database of six hundred entries, opened and dropped with the
//! system allocator underneath, leaves over two thousand readable copies of a
//! password in freed heap. With the allocator below the same measurement is
//! zero. It costs about six per cent of a parse and nothing measurable on a
//! save: a thousand entries read back in 5.2 ms against 4.9, and fifty
//! thousand in 259 ms against 247, well inside the budget in `spec.md`.
//!
//! macOS writes over a freed block of its own accord up to about sixteen
//! kilobytes, which is why a small database appears clean whatever Coffer does
//! and why the suite uses a crowded one. Linux zeroes nothing at any size.
//!
//! The allocator is declared here rather than in the window's crate so that it
//! covers the test binaries as well: the suite that asserts no value survives a
//! lock has to be running under the thing that makes that true.
//!
//! What it does not cover: anything Objective-C allocates, which is its own
//! malloc zone, and anything in another process. The clipboard and the webview
//! are both on the far side of that line, and both are said out loud in the
//! README rather than quietly assumed away.

use std::alloc::{GlobalAlloc, Layout, System};

struct Scrubbing;

// SAFETY: every method forwards to the system allocator, which is a correct
// `GlobalAlloc`, and passes back its pointers unchanged. The only addition is
// writing over a block that is about to be handed back, which no caller may
// read from any more.
unsafe impl GlobalAlloc for Scrubbing {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's layout, forwarded as it came.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the block is the caller's and is live until `System.dealloc`
        // below, so writing over the whole of it is writing over memory nobody
        // else holds.
        unsafe {
            std::ptr::write_bytes(pointer, 0, layout.size());
            // A store immediately before a free is a store into memory the
            // optimiser is entitled to treat as dead. The barrier is what stops
            // it being deleted; without it this whole module compiles to
            // nothing at all.
            zeroize::optimization_barrier(std::slice::from_raw_parts(pointer, layout.size()));
            System.dealloc(pointer, layout);
        }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // Growing in place is given up on purpose. `System::realloc` calls the
        // C library's, which may extend the block or may move it, and when it
        // moves it the old contents stay where they were. A vector that doubles
        // while a database is being read would leave the smaller half of that
        // database behind, and nothing would ever free it again.
        // SAFETY: `layout` describes the live block at `pointer`, and `size`
        // with the same alignment is a layout the caller has already checked.
        unsafe {
            let wanted = Layout::from_size_align_unchecked(size, layout.align());
            let fresh = System.alloc(wanted);
            if !fresh.is_null() {
                std::ptr::copy_nonoverlapping(pointer, fresh, layout.size().min(size));
                self.dealloc(pointer, layout);
            }
            fresh
        }
    }
}

#[global_allocator]
static ALLOCATOR: Scrubbing = Scrubbing;

/// Overwrites the stack this thread has just finished using.
///
/// Key derivation leaves the composite key, the transformed key and the master
/// key in fixed-size arrays that live on the stack, where no allocator can
/// reach them. This writes over the frames they were in.
///
/// Best effort, and deliberately named as such: how much stack a computation
/// used is an estimate, and `#[inline(never)]` on the function that used it is
/// a hint rather than a promise. Call it from the thread that derived the key,
/// after the work has returned.
pub fn stack() {
    // Half a megabyte covers Argon2's own frames with room to spare; the
    // sixty-four megabytes it works in are heap and are covered above.
    zeroize::zeroize_stack::<{ 512 * 1024 }>();
}
