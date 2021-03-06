// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[cfg(stage0)]
use core::prelude::v1::*;

use libc;
use self::imp::{make_handler, drop_handler};

pub use self::imp::{init, cleanup};

pub struct Handler {
    _data: *mut libc::c_void
}

impl Handler {
    pub unsafe fn new() -> Handler {
        make_handler()
    }
}

impl Drop for Handler {
    fn drop(&mut self) {
        unsafe {
            drop_handler(self);
        }
    }
}

#[cfg(any(target_os = "linux",
          target_os = "macos",
          target_os = "bitrig",
          target_os = "netbsd",
          target_os = "openbsd"))]
mod imp {
    use super::Handler;
    use rt::util::report_overflow;
    use mem;
    use ptr;
    use sys::c::{siginfo, sigaction, SIGBUS, SIG_DFL,
                 SA_SIGINFO, SA_ONSTACK, sigaltstack,
                 SIGSTKSZ, sighandler_t};
    use libc;
    use libc::funcs::posix88::mman::{mmap, munmap};
    use libc::{SIGSEGV, PROT_READ, PROT_WRITE, MAP_PRIVATE, MAP_ANON};
    use libc::MAP_FAILED;

    use sys_common::thread_info;


    // This is initialized in init() and only read from after
    static mut PAGE_SIZE: usize = 0;

    // Signal handler for the SIGSEGV and SIGBUS handlers. We've got guard pages
    // (unmapped pages) at the end of every thread's stack, so if a thread ends
    // up running into the guard page it'll trigger this handler. We want to
    // detect these cases and print out a helpful error saying that the stack
    // has overflowed. All other signals, however, should go back to what they
    // were originally supposed to do.
    //
    // This handler currently exists purely to print an informative message
    // whenever a thread overflows its stack. When run the handler always
    // un-registers itself after running and then returns (to allow the original
    // signal to be delivered again). By returning we're ensuring that segfaults
    // do indeed look like segfaults.
    //
    // Returning from this kind of signal handler is technically not defined to
    // work when reading the POSIX spec strictly, but in practice it turns out
    // many large systems and all implementations allow returning from a signal
    // handler to work. For a more detailed explanation see the comments on
    // #26458.
    unsafe extern fn signal_handler(signum: libc::c_int,
                                    info: *mut siginfo,
                                    _data: *mut libc::c_void) {
        let guard = thread_info::stack_guard().unwrap_or(0);
        let addr = (*info).si_addr as usize;

        // If the faulting address is within the guard page, then we print a
        // message saying so.
        if guard != 0 && guard - PAGE_SIZE <= addr && addr < guard {
            report_overflow();
        }

        // Unregister ourselves by reverting back to the default behavior.
        let mut action: sigaction = mem::zeroed();
        action.sa_sigaction = SIG_DFL;
        sigaction(signum, &action, ptr::null_mut());

        // See comment above for why this function returns.
    }

    static mut MAIN_ALTSTACK: *mut libc::c_void = 0 as *mut libc::c_void;

    pub unsafe fn init() {
        PAGE_SIZE = ::sys::os::page_size();

        let mut action: sigaction = mem::zeroed();
        action.sa_flags = SA_SIGINFO | SA_ONSTACK;
        action.sa_sigaction = signal_handler as sighandler_t;
        sigaction(SIGSEGV, &action, ptr::null_mut());
        sigaction(SIGBUS, &action, ptr::null_mut());

        let handler = make_handler();
        MAIN_ALTSTACK = handler._data;
        mem::forget(handler);
    }

    pub unsafe fn cleanup() {
        Handler { _data: MAIN_ALTSTACK };
    }

    pub unsafe fn make_handler() -> Handler {
        let alt_stack = mmap(ptr::null_mut(),
                             SIGSTKSZ,
                             PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANON,
                             -1,
                             0);
        if alt_stack == MAP_FAILED {
            panic!("failed to allocate an alternative stack");
        }

        let mut stack: sigaltstack = mem::zeroed();

        stack.ss_sp = alt_stack;
        stack.ss_flags = 0;
        stack.ss_size = SIGSTKSZ;

        sigaltstack(&stack, ptr::null_mut());

        Handler { _data: alt_stack }
    }

    pub unsafe fn drop_handler(handler: &mut Handler) {
        munmap(handler._data, SIGSTKSZ);
    }
}

#[cfg(not(any(target_os = "linux",
              target_os = "macos",
              target_os = "bitrig",
              target_os = "netbsd",
              target_os = "openbsd")))]
mod imp {
    use libc;

    pub unsafe fn init() {
    }

    pub unsafe fn cleanup() {
    }

    pub unsafe fn make_handler() -> super::Handler {
        super::Handler { _data: 0 as *mut libc::c_void }
    }

    pub unsafe fn drop_handler(_handler: &mut super::Handler) {
    }
}
