# Security Policy

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report privately to the maintainers via one of:

- Email: see the GitHub profile `@lthiagol`
- GitHub Security Advisories: `https://github.com/lthiagol/master-plan/security/advisories/new`

Include:

- A description of the vulnerability and its impact
- Steps to reproduce or a proof-of-concept
- Affected versions / commit SHAs
- Any known mitigations

## Response

- **Acknowledgement** within 72 hours of the report
- **Triage** within 7 days: confirm, prioritize, and propose a fix timeline
- **Fix** for confirmed vulnerabilities in the next release, or sooner for critical issues
- **Credit** to the reporter in the fix release notes (unless anonymity is requested)

## Scope

`mp` is an agent-only CLI; it executes on developer machines and CI runners.
The threat model treats the user's filesystem, environment variables, and
repository-controlled plan/config content as inputs that must not exceed the
invoking user's OS permissions. See `crates/mp/src/ac_verify.rs` and
`crates/mp/src/install.rs` for the trust boundaries.

## Supported versions

Only the latest released version on the `stable` branch receives security
fixes. The `wip` branch may receive fixes ahead of release but is not
considered stable for production use.