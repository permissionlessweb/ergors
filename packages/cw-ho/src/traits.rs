/// Generic trait for wrapping inner types with outer wrapper types
pub trait Wrap<Inner> {
    /// Wrap an inner type reference as a wrapper type reference
    fn wrap_ref(inner: &Inner) -> &Self;
    /// Wrap an inner type as a wrapper type
    fn wrap(inner: Inner) -> Self;
    /// Unwrap to get the inner type
    fn unwrap(self) -> Inner;
}

/// Macro to generate wrapper types with all necessary implementations
#[macro_export]
macro_rules! define_wrapper {
    ($wrapper:ident, $inner:ty) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $wrapper(pub $inner);

        impl Deref for $wrapper {
            type Target = $inner;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl $crate::traits::Wrap<$inner> for $wrapper {
            fn wrap_ref(inner: &$inner) -> &Self {
                // SAFETY: $wrapper is #[repr(transparent)] around $inner
                unsafe { &*(inner as *const $inner as *const $wrapper) }
            }

            fn wrap(inner: $inner) -> Self {
                $wrapper(inner)
            }

            fn unwrap(self) -> $inner {
                self.0
            }
        }

        impl AsRef<$wrapper> for $inner {
            fn as_ref(&self) -> &$wrapper {
                // SAFETY: $wrapper is #[repr(transparent)] around $inner
                unsafe { &*(self as *const $inner as *const $wrapper) }
            }
        }
    };
}
