use bumpalo::{collections::Vec, Bump};
use crossbeam::utils::{Backoff, CachePadded};

use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

// Concurrent access to the Bump allocator is prevented through the
// use of a simple atomics based spin loop.
unsafe impl Send for Allocator {}
unsafe impl Sync for Allocator {}

#[derive(Debug)]
pub struct Allocator {
    bump: Bump,
    lock: CachePadded<std::sync::atomic::AtomicBool>,
}

pub struct Mem {
    ptr: NonNull<u8>,
    cap: usize,
    len: usize,
}

pub enum RawAlloc {
    PathBuf(Mem),
    String(Mem),
}

impl Allocator {
    pub fn new() -> Allocator {
        Allocator {
            bump: Bump::new(),
            lock: CachePadded::new(AtomicBool::new(false)),
        }
    }

    #[inline]
    pub fn pathbuf(&self, size: usize) -> RawAlloc {
        self.lock();
        let vec = Vec::with_capacity_in(size, &self.bump);
        self.unlock();
        let mem = Mem {
            ptr: unsafe { NonNull::new_unchecked(vec.into_bump_slice_mut().as_mut_ptr()) },
            cap: size,
            len: 0,
        };

        RawAlloc::PathBuf(mem)
    }

    #[inline]
    pub fn string(&self, size: usize) -> RawAlloc {
        self.lock();
        let vec = Vec::with_capacity_in(size, &self.bump);
        self.unlock();
        let mem = Mem {
            ptr: unsafe { NonNull::new_unchecked(vec.into_bump_slice_mut().as_mut_ptr()) },
            cap: size,
            len: 0,
        };

        RawAlloc::String(mem)
    }

    // Only reason this ISN'T wrong is that capacity will always be
    // larger than size. Otherwise a race during reallocation could
    // cause bad things to happen.
    #[inline]
    pub fn tmpfiles(&self, cap: usize) -> Vec<crate::files::File> {
        self.lock();
        let vec = Vec::with_capacity_in(cap, &self.bump);
        self.unlock();
        vec
    }

    #[inline]
    pub fn dentbuf(&self, cap: usize) -> *mut u8 {
        let align = std::mem::align_of::<crate::files::linux_dirent>();
        let layout = std::alloc::Layout::from_size_align(cap, align).unwrap();
        self.lock();
        let buf = self.bump.alloc_layout(layout);
        self.unlock();
        buf.as_ptr()
    }

    #[inline]
    fn lock(&self) {
        let backoff = Backoff::new();
        while self.lock.compare_and_swap(false, true, Ordering::Acquire) {
            backoff.snooze();
        }
    }

    #[inline]
    fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }
}

impl RawAlloc {
    #[inline]
    fn get_mem(&mut self) -> &mut Mem {
        let mem = match self {
            RawAlloc::PathBuf(mem) => mem,
            RawAlloc::String(mem) => mem,
        };
        mem
    }

    #[inline]
    pub fn write(&mut self, src: &[u8]) {
        let mem = self.get_mem();
        let len = mem.len;
        let src_len = src.len();

        if (mem.len + src_len) > mem.cap {
            let mem_bytes = unsafe { std::slice::from_raw_parts(mem.ptr.as_ptr(), len) };

            let mem_string = String::from_utf8_lossy(mem_bytes);
            let src_string = String::from_utf8_lossy(src);
            panic!(
                "Can't write this much into {:?}\nWanted to add:{}",
                mem_string, src_string
            );
        };

        mem.len += src_len;

        unsafe {
            let ptr = mem.ptr.as_ptr().offset(len as isize);
            ptr.copy_from_nonoverlapping(src.as_ptr(), src_len)
        }
    }

    #[inline]
    pub fn finalize_pathbuf(mut self) -> PathBuf {
        let mem = self.get_mem();
        let len = mem.len;
        let cap = mem.cap;
        let ptr = mem.ptr.as_ptr();

        let pathbuf = unsafe {
            let mut stdvec = std::vec::Vec::from_raw_parts(ptr, len, cap);
            let ptr = &mut stdvec as *mut std::vec::Vec<u8>;
            std::mem::forget(stdvec);
            ptr.cast::<PathBuf>().read()
        };
        return pathbuf;
    }

    // #[inline]
    // #[target_feature(enable = "avx2")]
    // unsafe fn verify_string(&self) -> bool {
    //     if let RawAlloc::String(mem) = self {
    //         let ptr = mem.ptr.as_ptr();
    //         let len = mem.len;
    //         let slice = // unsafe {
    //             std::slice::from_raw_parts(ptr, len);
    //         //};

    //         let valid = is_utf8::lemire::avx::is_utf8_ascii_path(slice);
    //         return valid;

    //     } else { panic!("Called verify_string() on non-string allocation!"); }
    // }

    pub fn finalize_string(mut self) -> String {
        let mem = self.get_mem();
        let len = mem.len;
        let cap = mem.cap;
        let ptr = mem.ptr.as_ptr();

        let string = unsafe {
            // match self.verify_string() {
            // true =>
            String::from_raw_parts(ptr, len, cap) // ,
                                                  // false => {
                                                  //     let slice = std::slice::from_raw_parts(ptr, len);
                                                  //     String::from_utf8_lossy(slice).to_string()
                                                  // }
                                                  // }
        };

        return string;
    }
}

impl Drop for Allocator {
    fn drop(&mut self) {
        // if Arc::strong_count(&self.bump) <= 1 {
        self.bump.reset();
        // }
    }
}
