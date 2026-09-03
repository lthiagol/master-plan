//! Consolidated integration tests — reduces linker invocations.

mod common;

#[path = "suites/init_empty_collections.rs"]
mod init_empty_collections;

#[path = "suites/init_from_repo_markdown.rs"]
mod init_from_repo_markdown;

#[path = "suites/init_json.rs"]
mod init_json;

#[path = "suites/init_plan_dir.rs"]
mod init_plan_dir;

#[path = "suites/init_refresh.rs"]
mod init_refresh;

#[path = "suites/init_root_agents.rs"]
mod init_root_agents;

#[path = "suites/init_transactional.rs"]
mod init_transactional;
