use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

pub type Handle = NonZeroU32;

#[derive(PartialEq, Eq, Hash, Clone)]
pub struct VPath(Rc<(String, Option<VPath>)>);

impl VPath {
    pub fn new(key: &str) -> VPath {
        VPath(Rc::new((key.to_owned(), None)))
    }
    pub fn extend(&self, key: &str) -> VPath {
        VPath(Rc::new((key.to_owned(), Some(self.clone()))))
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

    pub fn from_prev(mut old: HandleTree<Value>) -> Self {
        old.obj_by_handle.clear();
        old.prev_handle_by_vpath.clear();

        HandleTree {
            obj_by_handle: old.obj_by_handle,
            handle_by_vpath: old.prev_handle_by_vpath,
            prev_handle_by_vpath: old.handle_by_vpath,
            next_handle_value: old.next_handle_value,
        }
    }

    pub fn create_handle(&mut self, parent_handle: Option<Handle>, key: &str, value: Value) -> Handle {
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
