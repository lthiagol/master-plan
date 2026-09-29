#!/usr/bin/env bash
# verify-m230-ac01.sh — M230 AC-01 source-pin (cycle 2 wrapper).
#
# Wraps the original 4 file / source-pin checks into a single
# argv-clean command so mp's argv-only verifier can run it at
# `mp milestone complete` time:
#
#   1. the legacy tui/watch.rs is gone,
#   2. the legacy tui/render/watch.rs is gone,
#   3. the new tui/render/autopilot_lane.rs exists,
#   4. no source/test reference names the old module /
#      function path / render_watch_lane entry point.
#
# Exit 0 when all four pass; non-zero on the first failure.
set -e
test ! -e crates/raul/src/tui/watch.rs
test ! -e crates/raul/src/tui/render/watch.rs
test -f crates/raul/src/tui/render/autopilot_lane.rs
! rg -q 'pub mod watch|tui::watch|render::watch|render_watch_lane' crates/raul/src crates/raul/tests