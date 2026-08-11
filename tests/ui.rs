//! Compile-fail contracts for diagnostics emitted by the public macros.

/// Keeps invalid Vue and Tailwind syntax from silently changing behavior.
#[test]
fn compile_fail_contracts() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/fail_*.rs");
}
