//! Reading this process's own memory back, to see what a lock left behind.
//!
//! The slice-4 criterion is that a process dump taken after an auto-lock holds
//! none of the test database's values. This is that dump, taken from inside.
//!
//! Two rules make the answer mean something. Nothing is ever read into a Rust
//! reference straight off an address: every byte comes back through the kernel,
//! which writes into a buffer this module owns, because a slice laid over
//! arbitrary mapped memory would be reading bytes no Rust value has
//! initialised. And the two buffers this module itself holds - the value being
//! looked for, and the one the kernel copies into - are cut out of every span
//! before it is searched, because a scanner that finds its own needle finds one
//! every time.

/// A value, and the machinery for counting how many copies of it this process
/// is still holding.
///
/// It owns both of its buffers so that it knows where they are: they are the
/// two places a copy is expected, and the only two that are not evidence.
pub struct Sweep {
    needle: Vec<u8>,
    /// Where the kernel writes. Allocated once, at a fixed address, so that it
    /// can be cut out of the search rather than chased around the heap.
    caught: Vec<u8>,
}

/// How much is asked for at a time. Small enough that a region which goes away
/// mid-sweep costs one failed read, large enough that a gigabyte of heap is not
/// sixteen thousand system calls.
const CHUNK: usize = 1 << 20;

impl Sweep {
    /// Takes the needle rather than borrowing it, so that there is exactly one
    /// copy of it in the process and this is it. A caller that kept its own
    /// would be handing the sweep a hit to find.
    pub fn for_needle(needle: Vec<u8>) -> Sweep {
        assert!(!needle.is_empty(), "an empty needle matches everywhere");
        Sweep {
            needle,
            caught: vec![0u8; CHUNK],
        }
    }

    /// How many copies of the needle are in this process's writable memory.
    ///
    /// Writable and not file-backed: a value that reached memory reached it
    /// through an allocation, and the read-only halves of the binary and of the
    /// shared cache cannot hold anything a database put there.
    pub fn hits(&mut self) -> usize {
        let mine = [
            span_of(&self.needle),
            (
                self.caught.as_ptr() as u64,
                self.caught.as_ptr() as u64 + self.caught.len() as u64,
            ),
        ];

        let mut found = 0;
        for region in writable() {
            for span in cut(region, &mine) {
                found += self.search(span);
            }
        }

        // What the last chunk left in the buffer is a copy of somebody's data,
        // and this module is not allowed to be the thing that leaves one behind.
        self.caught.fill(0);
        found
    }

    fn search(&mut self, (from, to): (u64, u64)) -> usize {
        let overlap = self.needle.len() as u64 - 1;
        let mut found = 0;
        let mut at = from;

        while at < to {
            let wanted = CHUNK.min((to - at) as usize);
            let Some(read) = read_into(at, &mut self.caught[..wanted]) else {
                // The region went away between being listed and being read.
                // There is nothing in it to count.
                break;
            };
            found += count(&self.caught[..read], &self.needle);

            if read < wanted || (read as u64) <= overlap {
                break;
            }
            // The next chunk starts inside this one, so a value lying across
            // the join is counted once rather than missed.
            at += read as u64 - overlap;
        }

        found
    }
}

fn span_of(bytes: &[u8]) -> (u64, u64) {
    let start = bytes.as_ptr() as u64;
    (start, start + bytes.len() as u64)
}

/// Everything in `region` that is not in one of `holes`.
fn cut(region: (u64, u64), holes: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut spans = vec![region];
    for &(from, to) in holes {
        let mut left = Vec::new();
        for (start, end) in spans {
            if to <= start || from >= end {
                left.push((start, end));
                continue;
            }
            if start < from {
                left.push((start, from));
            }
            if to < end {
                left.push((to, end));
            }
        }
        spans = left;
    }
    spans.into_iter().filter(|(from, to)| to > from).collect()
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    if haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::c_int;

    // The kernel's own declarations. `libc` carries the types and the constants
    // and not these three calls, and its `mach_task_self_` is deprecated in
    // favour of a crate this workspace is not going to add for five lines.
    unsafe extern "C" {
        static mach_task_self_: libc::mach_port_t;

        fn mach_vm_region(
            task: libc::vm_map_t,
            address: *mut libc::mach_vm_address_t,
            size: *mut libc::mach_vm_size_t,
            flavor: c_int,
            info: *mut c_int,
            count: *mut libc::mach_msg_type_number_t,
            object: *mut libc::mach_port_t,
        ) -> libc::kern_return_t;

        fn mach_vm_read_overwrite(
            task: libc::vm_map_t,
            address: libc::mach_vm_address_t,
            size: libc::mach_vm_size_t,
            into: libc::mach_vm_address_t,
            read: *mut libc::mach_vm_size_t,
        ) -> libc::kern_return_t;
    }

    /// `VM_REGION_BASIC_INFO_64` from `<mach/vm_region.h>`. The count that goes
    /// with it is the structure's own size in words and is never written down,
    /// so that adding a field cannot make the two disagree.
    const BASIC_INFO_64: c_int = 9;

    #[repr(C)]
    #[derive(Default)]
    struct BasicInfo64 {
        protection: libc::vm_prot_t,
        max_protection: libc::vm_prot_t,
        inheritance: libc::vm_inherit_t,
        shared: c_int,
        reserved: c_int,
        offset: libc::memory_object_offset_t,
        behavior: c_int,
        user_wired_count: libc::c_ushort,
    }

    pub fn writable() -> Vec<(u64, u64)> {
        // SAFETY: reading an integer the kernel exports.
        let task = unsafe { mach_task_self_ };
        let mut regions = Vec::new();
        let mut address: libc::mach_vm_address_t = 0;

        loop {
            let mut size: libc::mach_vm_size_t = 0;
            let mut info = BasicInfo64::default();
            let mut count =
                (size_of::<BasicInfo64>() / size_of::<c_int>()) as libc::mach_msg_type_number_t;
            let mut object: libc::mach_port_t = 0;

            // SAFETY: every pointer is to a local of the type the call expects,
            // and `count` says how many words `info` has room for.
            let outcome = unsafe {
                mach_vm_region(
                    task,
                    &mut address,
                    &mut size,
                    BASIC_INFO_64,
                    std::ptr::from_mut(&mut info).cast::<c_int>(),
                    &mut count,
                    &mut object,
                )
            };

            // Anything but success is the end of the map: there is no region at
            // or after this address.
            if outcome != libc::KERN_SUCCESS || size == 0 {
                break;
            }

            let wanted = libc::VM_PROT_READ | libc::VM_PROT_WRITE;
            if info.protection & wanted == wanted && info.shared == 0 {
                regions.push((address, address + size));
            }

            let Some(next) = address.checked_add(size) else {
                break;
            };
            address = next;
        }

        regions
    }

    pub fn read_into(address: u64, into: &mut [u8]) -> Option<usize> {
        let mut read: libc::mach_vm_size_t = 0;
        // SAFETY: `into` is a live buffer of `into.len()` bytes and the kernel
        // is told exactly that much room. Nothing here is dereferenced by this
        // process.
        let outcome = unsafe {
            mach_vm_read_overwrite(
                mach_task_self_,
                address,
                into.len() as libc::mach_vm_size_t,
                into.as_mut_ptr() as libc::mach_vm_address_t,
                &mut read,
            )
        };

        (outcome == libc::KERN_SUCCESS).then_some(read as usize)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::os::fd::AsRawFd;

    /// The regions `/proc/self/maps` says this process may write to.
    ///
    /// A mapping with a file behind it is skipped: a database's plaintext got
    /// into memory through an allocation, and everything named is either the
    /// binary or a library.
    pub fn writable() -> Vec<(u64, u64)> {
        let maps = std::fs::read_to_string("/proc/self/maps").expect("this process has a map");
        maps.lines()
            .filter_map(|line| {
                let mut columns = line.split_whitespace();
                let range = columns.next()?;
                let permissions = columns.next()?;
                if !permissions.starts_with("rw") {
                    return None;
                }
                // Address, offset, device, inode, then the name if there is one.
                let named = columns.nth(3).is_some_and(|name| name.starts_with('/'));
                if named {
                    return None;
                }
                let (from, to) = range.split_once('-')?;
                Some((
                    u64::from_str_radix(from, 16).ok()?,
                    u64::from_str_radix(to, 16).ok()?,
                ))
            })
            .collect()
    }

    pub fn read_into(address: u64, into: &mut [u8]) -> Option<usize> {
        use std::sync::OnceLock;

        static MEMORY: OnceLock<std::fs::File> = OnceLock::new();
        let file = MEMORY.get_or_init(|| {
            std::fs::File::open("/proc/self/mem").expect("this process can read itself")
        });

        // SAFETY: `into` is a live buffer of `into.len()` bytes, and `pread`
        // writes at most that many into it.
        let read = unsafe {
            libc::pread(
                file.as_raw_fd(),
                into.as_mut_ptr().cast::<libc::c_void>(),
                into.len(),
                address as libc::off_t,
            )
        };

        (read > 0).then_some(read as usize)
    }
}

use platform::{read_into, writable};
