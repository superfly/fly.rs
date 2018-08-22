// macro_rules! value_type {
//   ($typ:ident) => {
//     impl JSValue for $typ {
//       fn from_raw(raw: *const js_value) -> $typ {
//         $typ(raw)
//       }

//       fn as_raw(&self) -> *const js_value {
//         self.0
//       }
//     }

//     impl Drop for $typ {
//       fn drop(&mut self) {
//         debug!("Dropping value!");
//         unsafe { js_value_release(self.as_raw()) }
//       }
//     }
//   };
// }

// /// Implements a relationship between two subtypes.
// macro_rules! subtype {
//   ($child:ident, $parent:ident) => {
//     impl From<$child> for $parent {
//       fn from(child: $child) -> $parent {
//         unsafe { ::std::mem::transmute(child) }
//       }
//     }
//   };
// }

// macro_rules! inherit {
//   ($child:ident, $parent:ident) => {
//     subtype!($child, $parent);

//     impl ::std::ops::Deref for $child {
//       type Target = $parent;

//       fn deref(&self) -> &Self::Target {
//         unsafe { ::std::mem::transmute(self) }
//       }
//     }
//   };
// }
