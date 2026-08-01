#!/bin/bash
# M152 S5 integration-test gate. Each line is runnable on its
# own; this script is the bundled form referenced by the S5
# `tests` field. Lives in the workspace under scripts/ so
# `mp milestone step done S5`'s eval picks it up; copy-pastable
# for ad-hoc invocation.
set -euo pipefail
cargo nextest run -p mp --test watch_resume        --no-fail-fast
cargo nextest run -p mp --test watch_state_file   --no-fail-fast
cargo nextest run -p mp --test watch_no_double_spawn --no-fail-fast
cargo nextest run -p mp --test watch_signal        --no-fail-fast
