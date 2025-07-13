//! Zero-cost abstractions for compile-time optimizations
//!
//! This module provides type-safe, zero-cost abstractions that move runtime
//! checks to compile time, eliminating overhead while improving safety.
//!
//! ## Design Principles
//!
//! 1. **Zero Runtime Overhead**: All abstractions compile down to the same
//!    machine code as manual implementations
//! 2. **Compile-Time Safety**: Move validations and state checks to type system
//! 3. **Const Evaluation**: Use const generics and const functions for
//!    compile-time computations
//! 4. **Type State Pattern**: Encode state machines in the type system
//! 5. **Phantom Types**: Add compile-time constraints without runtime cost

pub mod type_state;
pub mod const_generics;
pub mod phantom_types;
pub mod validated_types;
pub mod static_dispatch;
pub mod compile_time_validation;
pub mod zero_alloc_patterns;
pub mod const_collections;
// pub mod blixard_integration; // Temporarily disabled due to compilation issues
pub mod simple_demo;

pub use type_state::{TypeState, StateTransition};
pub use const_generics::{ConstBuffer, ConstString, FixedCapacityVec};
pub use phantom_types::{Branded, NotSend, NotSync, PhantomState};
pub use validated_types::{NonEmpty, Positive, ValidatedString, BoundedInt};
pub use static_dispatch::{StaticDispatch, CompileTimeRegistry};
pub use compile_time_validation::{const_validate};
pub use zero_alloc_patterns::{StackString, InlineVec, StaticPool};
pub use const_collections::{ConstMap, ConstSet, StaticLookup};

/// Macro for creating zero-cost newtypes with validation
#[macro_export]
macro_rules! validated_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident($inner:ty) where |$val:ident| $validation:expr;
        $error_msg:literal
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        $vis struct $name($inner);

        impl $name {
            /// Creates a new instance if validation passes
            #[inline]
            pub const fn new($val: $inner) -> Result<Self, &'static str> {
                if $validation {
                    Ok(Self($val))
                } else {
                    Err($error_msg)
                }
            }

            /// Creates a new instance, panicking if validation fails
            #[inline]
            pub const fn new_unchecked($val: $inner) -> Self {
                assert!($validation, $error_msg);
                Self($val)
            }

            /// Returns the inner value
            #[inline]
            pub const fn into_inner(self) -> $inner {
                self.0
            }

            /// Returns a reference to the inner value
            #[inline]
            pub const fn as_inner(&self) -> &$inner {
                &self.0
            }
        }

        impl std::ops::Deref for $name {
            type Target = $inner;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

/// Macro for creating const functions with compile-time assertions
#[macro_export]
macro_rules! const_fn_with_assert {
    (
        $(#[$meta:meta])*
        $vis:vis const fn $name:ident $(<$($generic:tt)*>)? (
            $($arg:ident: $arg_ty:ty),*
        ) -> $ret:ty
        where
            const { $($assert:expr);* }
        $body:block
    ) => {
        $(#[$meta])*
        $vis const fn $name $(<$($generic)*>)? ($($arg: $arg_ty),*) -> $ret {
            $(
                const _: () = assert!($assert);
            )*
            $body
        }
    };
}

/// Macro for creating type-state machines
#[macro_export]
macro_rules! type_state_machine {
    (
        $(#[$meta:meta])*
        $vis:vis machine $name:ident {
            states {
                $($state:ident $(($($field:ident: $field_ty:ty),*))? => {
                    $($transition:ident -> $target:ident)?
                }),*
            }
        }
    ) => {
        $(#[$meta])*
        $vis mod $name {
            use super::*;

            /// Marker trait for states
            pub trait State: private::Sealed {}

            /// The state machine
            pub struct Machine<S: State> {
                _state: std::marker::PhantomData<S>,
                #[allow(dead_code)]
                shared_data: std::sync::Arc<SharedData>,
            }

            struct SharedData {
                // Shared fields across all states
            }

            mod private {
                pub trait Sealed {}
            }

            $(
                /// State: $state
                pub struct $state {
                    $($($field: $field_ty,)*)?
                }

                impl private::Sealed for $state {}
                impl State for $state {}

                impl Machine<$state> {
                    $(
                        /// Transition from $state to $target
                        pub fn $transition(self) -> Machine<$target> {
                            Machine {
                                _state: std::marker::PhantomData,
                                shared_data: self.shared_data,
                            }
                        }
                    )?
                }
            )*
        }
    };
}