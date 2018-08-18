#![allow(non_upper_case_globals)]

#[macro_use]
extern crate cpp;
#[macro_use]
extern crate bitflags;

use std::ffi::{CStr, CString};
use std::fmt;
use std::mem;
use std::os::raw::c_char;
//use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::str;

cpp!{{
    #include <lldb/API/LLDB.h>
    using namespace lldb;
}}

pub type Address = u64;
pub type ThreadID = u64;
pub type BreakpointID = u32;
pub type UserID = u64;

/////////////////////////////////////////////////////////////////////////////////////////////////////

fn with_cstr<R, F>(s: &str, f: F) -> R
where
    F: FnOnce(*const i8) -> R,
{
    let allocated;
    let mut buffer: [u8; 256] = unsafe { mem::uninitialized() };
    let ptr: *const i8 = if s.len() < buffer.len() {
        buffer[0..s.len()].clone_from_slice(s.as_bytes());
        buffer[s.len()] = 0;
        buffer.as_ptr() as *const i8
    } else {
        allocated = Some(CString::new(s).unwrap());
        allocated.as_ref().unwrap().as_ptr()
    };
    f(ptr)
}

fn with_opt_cstr<R, F>(s: Option<&str>, f: F) -> R
where
    F: FnOnce(*const i8) -> R,
{
    match s {
        Some(s) => with_cstr(s, f),
        None => f(ptr::null()),
    }
}

fn get_string<F>(mut capacity_hint: usize, f: F) -> String
where
    F: Fn(*mut c_char, usize) -> usize,
{
    // Note that some API return the required size of the full string (SBThread::GetStopDescription()),
    // while others return the number of bytes actually written into the buffer (SBPath::GetPath()).
    // There also seems to be lack of consensus on whether the terminating NUL should be included in the count...
    // So we take the conservative approach:
    // - if returned size >= (size of buffer - 1), we retry with a bigger buffer,
    // - count bytes to NUL ourselves.

    // Use static buffer if the string is likely to fit
    let buffer: [u8; 256] = unsafe { mem::uninitialized() };
    if capacity_hint <= buffer.len() {
        let c_ptr = buffer.as_ptr() as *mut c_char;
        let size = f(c_ptr, buffer.len());
        assert!((size as isize) >= 0);
        if size < buffer.len() - 1 {
            unsafe {
                let s = CStr::from_ptr(c_ptr).to_str().unwrap(); // Count bytes to NUL
                return s.to_owned();
            }
        }
        capacity_hint = if size > buffer.len() { size + 1 } else { buffer.len() * 2 };
    }

    let mut buffer = Vec::with_capacity(capacity_hint);
    loop {
        let c_ptr = buffer.as_ptr() as *mut c_char;
        let size = f(c_ptr, buffer.capacity());
        assert!((size as isize) >= 0);
        if size < buffer.capacity() - 1 {
            unsafe {
                let s = CStr::from_ptr(c_ptr).to_str().unwrap();
                buffer.set_len(s.len());
                return String::from_utf8_unchecked(buffer);
            };
        }
        capacity_hint = if size > capacity_hint { size + 1 } else { capacity_hint * 2 };
        let additional = capacity_hint - buffer.capacity();
        buffer.reserve(additional);
        //assert!(buffer.capacity() >= capacity_hint);
    }
}

fn debug_descr<CPP>(f: &mut fmt::Formatter, cpp: CPP) -> fmt::Result
where
    CPP: FnOnce(&mut SBStream) -> bool,
{
    let mut descr = SBStream::new();
    if cpp(&mut descr) {
        match str::from_utf8(descr.data()) {
            Ok(s) => f.write_str(s),
            Err(_) => Err(fmt::Error),
        }
    } else {
        Ok(())
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

struct SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    size: u32,
    get_item: GetItem,
    index: u32,
}

impl<Item, GetItem> SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    fn new(size: u32, get_item: GetItem) -> Self {
        Self {
            size: size,
            get_item: get_item,
            index: 0,
        }
    }
}

impl<Item, GetItem> Iterator for SBIterator<Item, GetItem>
where
    GetItem: FnMut(u32) -> Item,
{
    type Item = Item;
    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.size {
            self.index += 1;
            Some((self.get_item)(self.index - 1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        return (0, Some(self.size as usize));
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

mod sbaddress;
mod sbbreakpoint;
mod sbbreakpointlocation;
mod sbbroadcaster;
mod sbcommandinterpreter;
mod sbcommandreturnobject;
mod sbdata;
mod sbdebugger;
mod sberror;
mod sbevent;
mod sbexecutioncontext;
mod sbfilespec;
mod sbframe;
mod sbinstruction;
mod sbinstructionlist;
mod sblaunchinfo;
mod sblinenetry;
mod sblistener;
mod sbmodule;
mod sbprocess;
mod sbstream;
mod sbsymbol;
mod sbtarget;
mod sbthread;
mod sbtype;
mod sbvalue;
mod sbvaluelist;

pub use sbaddress::*;
pub use sbbreakpoint::*;
pub use sbbreakpointlocation::*;
pub use sbbroadcaster::*;
pub use sbcommandinterpreter::*;
pub use sbcommandreturnobject::*;
pub use sbdata::*;
pub use sbdebugger::*;
pub use sberror::*;
pub use sbevent::*;
pub use sbexecutioncontext::*;
pub use sbfilespec::*;
pub use sbframe::*;
pub use sbinstruction::*;
pub use sbinstructionlist::*;
pub use sblaunchinfo::*;
pub use sblinenetry::*;
pub use sblistener::*;
pub use sbmodule::*;
pub use sbprocess::*;
pub use sbstream::*;
pub use sbsymbol::*;
pub use sbtarget::*;
pub use sbthread::*;
pub use sbtype::*;
pub use sbvalue::*;
pub use sbvaluelist::*;

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use super::*;

    fn store_as_cstr(s: &str, buff: *mut c_char, size: usize) -> usize {
        let b = unsafe { slice::from_raw_parts_mut(buff as *mut u8, size) };
        let s = s.as_bytes();
        if b.len() > s.len() {
            b[..s.len()].clone_from_slice(s);
            b[s.len()] = 0;
            s.len()
        } else {
            let max = b.len() - 1;
            b[..max].clone_from_slice(&s[..max]);
            b[max] = 0;
            max
        }
    }

    #[test]
    fn test_get_string() {
        use std::cell::RefCell;
        for n in 0..200 {
            let string = "0123456789ABC".repeat(n);
            for &hint in &[0, 50, 300] {
                let iters = RefCell::new(0..100);
                assert_eq!(
                    string,
                    get_string(hint, |buff, size| {
                        assert!(iters.borrow_mut().next().is_some());
                        store_as_cstr(&string, buff, size);
                        // Returns the required storage length
                        string.len()
                    })
                );
                let iters = RefCell::new(0..100);
                assert_eq!(
                    string,
                    get_string(hint, |buff, size| {
                        assert!(iters.borrow_mut().next().is_some());
                        store_as_cstr(&string, buff, size);
                        // Returns the required storage length, including NUL
                        string.len() + 1
                    })
                );
                let iters = RefCell::new(0..100);
                assert_eq!(
                    string,
                    get_string(hint, |buff, size| {
                        assert!(iters.borrow_mut().next().is_some());
                        // Returns stored length
                        store_as_cstr(&string, buff, size)
                    })
                );
                let iters = RefCell::new(0..100);
                assert_eq!(
                    string,
                    get_string(hint, |buff, size| {
                        assert!(iters.borrow_mut().next().is_some());
                        // Returns stored length, including NUL
                        store_as_cstr(&string, buff, size) + 1
                    })
                );
            }
        }
    }
}
