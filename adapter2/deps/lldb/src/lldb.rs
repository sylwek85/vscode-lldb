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

fn get_string<F>(initial_capacity: usize, f: F) -> String
where
    F: Fn(*mut c_char, usize) -> usize,
{
    let mut buffer = Vec::with_capacity(initial_capacity);
    let mut size = f(buffer.as_ptr() as *mut c_char, buffer.capacity());
    assert!((size as isize) >= 0);

    if size >= buffer.capacity() {
        let additional = size - buffer.capacity() + 1;
        buffer.reserve(additional);
        size = f(buffer.as_ptr() as *mut c_char, buffer.capacity());
        assert!((size as isize) >= 0);
    }
    unsafe { buffer.set_len(size) };
    String::from_utf8(buffer).unwrap()
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
