use std::cell::UnsafeCell;
use std::fmt;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

pub struct SingleBorrowRefCell<T: Sized> {
    borrow: UnsafeCell<bool>,
    value: UnsafeCell<T>,
}

unsafe impl<T: Sized + Send + Sync> Send for SingleBorrowRefCell<T> {}
unsafe impl<T: Sized + Send + Sync> Sync for SingleBorrowRefCell<T> {}

impl<T: Sized> SingleBorrowRefCell<T> {
    pub fn new(value: T) -> Self {
        Self {
            borrow: UnsafeCell::new(false),
            value: UnsafeCell::new(value),
        }
    }

    fn mark_borrowed(&self) {
        unsafe {
            *self.borrow.get() = true;
        }
    }

    fn mark_unborrowed(&self) {
        unsafe {
            *self.borrow.get() = false;
        }
    }

    pub fn borrow(&self) -> SingleBorrowRef<T> {
        if unsafe { *self.borrow.get() } {
            panic!("Already borrowed");
        }
        self.mark_borrowed();
        SingleBorrowRef {
            cell: self,
            value: unsafe { &mut *self.value.get() },
        }
    }
}

pub struct SingleBorrowRef<'a, T: Sized + 'a> {
    value: &'a mut T,
    cell: &'a SingleBorrowRefCell<T>,
}

impl<'a, T: Sized> Drop for SingleBorrowRef<'a, T> {
    fn drop(&mut self) {
        self.cell.mark_unborrowed();
    }
}

impl<'a, T: Sized> Deref for SingleBorrowRef<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T: Sized> DerefMut for SingleBorrowRef<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T: Sized + Debug + 'a> Debug for SingleBorrowRef<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: Sized + Debug> Debug for SingleBorrowRefCell<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SingleBorrowRefCell {{ ... }}")
    }
}
