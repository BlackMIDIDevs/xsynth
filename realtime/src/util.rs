use std::cell::UnsafeCell;

pub struct ReadWriteAtomicU64(UnsafeCell<u64>);

impl ReadWriteAtomicU64 {
    pub fn new(value: u64) -> Self {
        ReadWriteAtomicU64(UnsafeCell::new(value))
    }

    pub fn read(&self) -> u64 {
        unsafe { *self.0.get() }
    }

    pub fn write(&self, value: u64) {
        unsafe { *self.0.get() = value }
    }
}

unsafe impl Send for ReadWriteAtomicU64 {}
unsafe impl Sync for ReadWriteAtomicU64 {}
