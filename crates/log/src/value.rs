//! `stack-dst` like Value to put trait objects into a global.

use core::marker::{PhantomData, Unsize};
use core::ptr::{self, DynMetadata, Pointee};
use core::{mem, ops};

/// Stack-Allocated dynamically sized type.
///
/// `T` is the trait object type
/// `N` is the number of `usize`s used to store the data and info
pub struct Value<T, const N: usize>
where
    T: ?Sized + Pointee<Metadata = DynMetadata<T>>,
{
    // force alignment to be 8 bytes
    _align: [u64; 0],
    _type: PhantomData<T>,
    data: [usize; N],
}

impl<T, const N: usize> Value<T, N>
where
    T: ?Sized + Pointee<Metadata = DynMetadata<T>>,
{
    /// Create a new stack-allocated DST.
    pub fn new<U: Unsize<T>>(mut val: U) -> Result<Self, U> {
        let rv = {
            let ptr: *const T = &val;
            let meta = ptr::metadata(ptr);

            // make sure the value can be aligned to us
            assert!(
                mem::align_of::<U>() <= mem::align_of::<Self>(),
                "the value must be aligned to {} bytes or less",
                mem::align_of::<Self>(),
            );

            unsafe { Value::new_raw(meta, &mut val as *mut _ as *mut (), mem::size_of::<U>()) }
        };

        match rv {
            Some(r) => {
                // prevent the destructor from running, since the data has
                // been copied into this values buffer
                mem::forget(val);
                Ok(r)
            }
            None => Err(val),
        }
    }

    unsafe fn new_raw(info: DynMetadata<T>, data: *mut (), size: usize) -> Option<Value<T, N>> {
        let info_size = mem::size_of_val(&info);
        let info_word_size = info_size / mem::size_of::<usize>();

        assert!(
            info_word_size != 0 && info_size % mem::size_of::<usize>() == 0,
            "pointer metadata must be at least one word size and a multiple of word sizes"
        );

        if info_size + size > (N * mem::size_of::<usize>()) {
            None
        } else {
            let mut val = Value {
                _align: [],
                _type: PhantomData,
                data: [0usize; N],
            };

            // place pointer information at the end of the region
            // required to place the data at an aligned address
            let info_off = val.data.len() - info_word_size;
            let info_dst = val.data.as_mut_ptr().add(info_off);

            // copy the vtable
            ptr::copy_nonoverlapping(&info as *const _ as *const usize, info_dst, info_word_size);

            // copy the actual data
            ptr::copy_nonoverlapping(data.cast::<u8>(), val.data.as_mut_ptr().cast(), size);
            Some(val)
        }
    }

    fn get_vtable(&self) -> DynMetadata<T> {
        let offset = self.data.len() - (mem::size_of::<DynMetadata<T>>() / mem::size_of::<usize>());

        unsafe { *self.data.as_ptr().add(offset).cast::<DynMetadata<T>>() }
    }
}

impl<T, const N: usize> ops::Deref for Value<T, N>
where
    T: ?Sized + Pointee<Metadata = DynMetadata<T>>,
{
    type Target = T;

    fn deref(&self) -> &T {
        let vtable = self.get_vtable();
        unsafe { &*ptr::from_raw_parts(self.data.as_ptr().cast(), vtable) }
    }
}

impl<T, const N: usize> ops::DerefMut for Value<T, N>
where
    T: ?Sized + Pointee<Metadata = DynMetadata<T>>,
{
    fn deref_mut(&mut self) -> &mut T {
        let vtable = self.get_vtable();
        unsafe { &mut *ptr::from_raw_parts_mut(self.data.as_mut_ptr().cast(), vtable) }
    }
}

impl<T, const N: usize> Drop for Value<T, N>
where
    T: ?Sized + Pointee<Metadata = DynMetadata<T>>,
{
    fn drop(&mut self) {
        unsafe { ptr::drop_in_place::<T>(&mut **self) }
    }
}
