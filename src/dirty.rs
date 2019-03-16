#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct DirtyBit(bool);

pub trait Dirtyable {
    fn get_bit(&self) -> &DirtyBit;
    fn get_bit_mut(&mut self) -> &mut DirtyBit;

    fn is_dirty(&self) -> bool {
        self.get_bit().0
    }

    fn set_dirty(&mut self) {
        self.get_bit_mut().0 = true;
    }
    fn set_clean(&mut self) {
        self.get_bit_mut().0 = false;
    }
}


impl DirtyBit {
    pub fn new() -> DirtyBit {
        DirtyBit(false)
    }
}


impl Dirtyable for DirtyBit {
    fn get_bit(&self) -> &DirtyBit {
        self
    }
    fn get_bit_mut(&mut self) -> &mut DirtyBit {
        self
    }
}
