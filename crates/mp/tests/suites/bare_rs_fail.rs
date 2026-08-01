//! Failing integration-test binary invoked only via completion guardrail
//! (ignored-only binary re-run with `--include-ignored`).

#[test]
#[ignore = "guardrail fixture — not part of default suite"]
fn intentional_failure() {
    panic!("intentional failure for bare-path guardrail regression");
}
