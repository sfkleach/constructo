//! Generating code from the declarations of a project. The model describes the declarations in the same way for every
//! target, and each target writes its code from it.

mod graph;
pub mod model;
mod recursion;
pub mod rust;
