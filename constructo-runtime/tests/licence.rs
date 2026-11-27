//! The crate has its own copy of the licence, since Cargo packages only the files of the crate's folder, and the MIT
//! licence asks that its notice go with the code. This checks that the copy is the same as the repository's.
//!
//! The tests of this crate are in the repository and not in the published crate, so the path out of the crate's folder
//! is always there when this is compiled.

#[test]
fn the_licence_is_the_same_as_the_repository_s() {
    assert_eq!(
        include_str!("../LICENSE"),
        include_str!("../../LICENSE"),
        "constructo-runtime/LICENSE differs from LICENSE; copy LICENSE over it"
    );
}
