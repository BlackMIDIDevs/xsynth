use std::cell::UnsafeCell;
use std::ops::RangeInclusive;

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

pub struct ReadWriteAtomicRange(UnsafeCell<(u8, u8)>);

impl ReadWriteAtomicRange {
    pub fn new(range: RangeInclusive<u8>) -> Self {
        ReadWriteAtomicRange(UnsafeCell::new((*range.start(), *range.end())))
    }

    pub fn read(&self) -> RangeInclusive<u8> {
        let values = unsafe { *self.0.get() };
        values.0..=values.1
    }

    pub fn write(&self, range: RangeInclusive<u8>) {
        unsafe { *self.0.get() = (*range.start(), *range.end()) }
    }
}

unsafe impl Send for ReadWriteAtomicRange {}
unsafe impl Sync for ReadWriteAtomicRange {}
