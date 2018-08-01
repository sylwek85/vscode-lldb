use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::mem;
use std::num::NonZeroU32;
use std::rc::Rc;

pub type Handle = NonZeroU32;

pub fn to_i64(h: Option<Handle>) -> i64 {
    match h {
        None => 0,
        Some(v) => v.get() as i64,
    }
}

pub fn from_i64(v: i64) -> Option<Handle> {
    Handle::new(v as u32)
}

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct VPath(pub Rc<(Option<VPath>, String)>);

impl VPath {
    pub fn new(key: &str) -> VPath {
        VPath(Rc::new((None, key.to_owned())))
    }
    pub fn extend(&self, key: &str) -> VPath {
        VPath(Rc::new((Some(self.clone()), key.to_owned())))
    }
}

pub struct HandleTree<Value> {
    obj_by_handle: HashMap<Handle, (Value, VPath)>,
    handle_by_vpath: HashMap<VPath, Handle>,
    prev_handle_by_vpath: HashMap<VPath, Handle>,
    next_handle_value: u32,
}

impl<Value> HandleTree<Value> {
    pub fn new() -> Self {
        HandleTree {
            obj_by_handle: HashMap::new(),
            handle_by_vpath: HashMap::new(),
            prev_handle_by_vpath: HashMap::new(),
            next_handle_value: 1000,
        }
    }

    pub fn reset(&mut self) {
        self.obj_by_handle.clear();
        self.prev_handle_by_vpath.clear();
        let HandleTree {
            ref mut handle_by_vpath,
            ref mut prev_handle_by_vpath,
            ..
        } = self;
        mem::swap(handle_by_vpath, prev_handle_by_vpath);
    }

    pub fn create(&mut self, parent_handle: Option<Handle>, key: &str, value: Value) -> Handle {
        let new_vpath = match parent_handle {
            Some(parent_handle) => {
                let (_, parent_vpath) = self.obj_by_handle.get(&parent_handle).unwrap();
                parent_vpath.extend(key)
            }
            None => VPath::new(key),
        };

        let new_handle = match self.prev_handle_by_vpath.get(&new_vpath) {
            Some(handle) => handle.clone(),
            None => {
                self.next_handle_value += 1;
                Handle::new(self.next_handle_value).unwrap()
            }
        };

        self.handle_by_vpath.insert(new_vpath.clone(), new_handle);
        self.obj_by_handle.insert(new_handle, (value, new_vpath));
        new_handle
    }

    pub fn get(&self, handle: Handle) -> Option<&Value> {
        self.obj_by_handle.get(&handle).map(|t| &t.0)
    }

    pub fn get_with_vpath(&self, handle: Handle) -> Option<&(Value, VPath)> {
        self.obj_by_handle.get(&handle)
    }
}

#[test]
fn test1() {
    let mut handles = HandleTree::new();
    let a1 = handles.create(None, "1", 0xa1);
    let a2 = handles.create(None, "2", 0xa2);
    let a11 = handles.create(Some(a1), "1.1", 0xa11);
    let a12 = handles.create(Some(a1), "1.2", 0xa12);
    let a121 = handles.create(Some(a12), "1.2.1", 0xa121);
    let a21 = handles.create(Some(a2), "2.1", 0xa21);

    assert!(handles.get(a1).unwrap() == &0xa1);
    assert!(handles.get(a12).unwrap() == &0xa12);
    assert!(handles.get(a121).unwrap() == &0xa121);

    handles.reset();
    let b1 = handles.create(None, "1", 0xb1);
    let b3 = handles.create(None, "3", 0xb3);
    let b11 = handles.create(Some(b1), "1.1", 0xb11);
    let b12 = handles.create(Some(b1), "1.2", 0xb12);
    let b13 = handles.create(Some(b1), "1.3", 0xb13);
    let b121 = handles.create(Some(b12), "1.2.1", 0xb121);
    let b122 = handles.create(Some(b12), "1.2.2", 0xb122);

    assert!(handles.get(a2) == None);
    assert!(handles.get(a21) == None);

    assert!(b1 == a1);
    assert!(b11 == a11);
    assert!(b12 == a12);
    assert!(b121 == a121);

    assert!(handles.get(b1).unwrap() == &0xb1);
    assert!(handles.get(b122).unwrap() == &0xb122);
}

#[test]
#[should_panic]
fn test2() {
    let mut handles = HandleTree::new();
    let h1 = handles.create(None, "12345", 12345);
    let h2 = handles.create(Some(Handle::new(h1.get() + 1).unwrap()), "12345", 12345);
}
