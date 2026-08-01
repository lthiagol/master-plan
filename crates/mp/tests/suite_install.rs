//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/harness_registry.rs"]
mod harness_registry;

#[path = "suites/install_deploy.rs"]
mod install_deploy;

#[path = "suites/install_env_snippet.rs"]
mod install_env_snippet;

#[path = "suites/install_integrity.rs"]
mod install_integrity;

#[path = "suites/install_registry.rs"]
mod install_registry;

#[path = "suites/install_source.rs"]
mod install_source;

#[path = "suites/make_install_parity.rs"]
mod make_install_parity;

#[path = "suites/p6_install_bootstrap.rs"]
mod p6_install_bootstrap;
