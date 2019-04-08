use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct DirtyBit(bool);

#[derive(Debug)]
pub struct AsyncDirtyBit(pub Arc<RwLock<DirtyBit>>);

impl PartialEq for AsyncDirtyBit {
    fn eq(&self, other: &AsyncDirtyBit) -> bool {
        *self.0.read().unwrap() == *other.0.read().unwrap()
    }
}

impl Eq for AsyncDirtyBit {}

impl Hash for AsyncDirtyBit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.read().unwrap().hash(state)
    }
}

impl Clone for AsyncDirtyBit {
    fn clone(&self) -> Self {
        AsyncDirtyBit(self.0.clone())
    }
}

pub trait Dirtyable {
    fn is_dirty(&self) -> bool;
    fn set_dirty(&mut self);
    fn set_clean(&mut self);
}

impl DirtyBit {
    pub fn new() -> DirtyBit {
        DirtyBit(false)
    }
}

impl AsyncDirtyBit {
    pub fn new() -> AsyncDirtyBit {
        AsyncDirtyBit(Arc::new(RwLock::new(DirtyBit::new())))
    }
}

impl Dirtyable for DirtyBit {
    fn is_dirty(&self) -> bool {
        self.0
    }
    fn set_dirty(&mut self) {
        self.0 = true;
    }
    fn set_clean(&mut self) {
        self.0 = false;
    }
}

impl Dirtyable for AsyncDirtyBit {
    fn is_dirty(&self) -> bool {
        self.0.read().unwrap().is_dirty()
    }
    fn set_dirty(&mut self) {
        self.0.write().unwrap().set_dirty();
    }
    fn set_clean(&mut self) {
        self.0.write().unwrap().set_clean();
    }
}
