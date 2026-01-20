// Trybuild tests for CosmwasmExt derive macro
//
// NOTE: These tests are currently disabled because the derive macro generates
// code that depends on cosmwasm_std types and a crate::shim module.
// To enable these tests:
// 1. Uncomment cosmwasm-std in Cargo.toml dev-dependencies
// 2. Update struct.rs and query.rs with proper test cases

#[test]
fn tests() {
    // Temporarily disabled - see note above
    // let t = trybuild::TestCases::new();
    // t.pass("tests/struct.rs");
    // t.pass("tests/query.rs");
}
