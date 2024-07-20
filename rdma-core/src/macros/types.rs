macro_rules! rdma_type {
    ($wrapper_name: ident, $inner_type:ty) => {
        #[allow(non_snake_case)]
        pub mod $wrapper_name {
            use std::{
                cell::UnsafeCell,
                ops::{Deref, DerefMut},
                rc::Rc,
            };
            
            #[derive(Default)]
            struct Inner(Box<$inner_type>);

            #[derive(Debug, Clone)]
            pub struct $wrapper_name {
                inner: Rc<UnsafeCell<Inner>>,
            }

            impl $wrapper_name {
                pub fn new(inner: *mut $inner_type) -> $wrapper_name {
                    let inner = Rc::new(UnsafeCell::new(Inner(unsafe { Box::from_raw(inner) })));
                    $wrapper_name {
                        inner,
                    }
                }
            }

            impl From<*mut $inner_type> for $wrapper_name {
                fn from(value: *mut $inner_type) -> Self {
                    $wrapper_name::new(value)
                }
            }

            impl Deref for $wrapper_name {
                type Target = $inner_type;
                fn deref(&self) -> &Self::Target {
                    unsafe { &*self.inner.get() }.0.as_ref()
                }
            }
            
            impl DerefMut for $wrapper_name {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    unsafe { &mut *self.inner.get() }.0.as_mut()
                }
            }

            impl Default for $wrapper_name {
                fn default() -> $wrapper_name {
                    $wrapper_name{
                        inner: Rc::new(UnsafeCell::new(Default::default()))
                    }
                }
            }

        }
    }
}

pub(crate)  use rdma_type;
