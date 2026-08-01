# v2.0.0 — Master Plan Toolkit 2.0 GA

The toolkit's first GA under the agent-first design. JSON-canonical plan
storage, lean spec model, fragment-first agent I/O, search v2, top-tab raul
TUI, and size-aware positioning docs.

## Highlights

- **JSON only on disk.** No back-compat shim for TOML plans; `mp migrate`
  is the one-shot path for legacy projects.
- **Lean spec (M82).** Eighteen ceremony fields dropped from the milestone
  schema. `mp validate` enforces the lean schema.
- **Fragment-first agent I/O (M93).** `mp milestone {ac,step,wp} {show,update,remove}
  <id> [<fragment-id>]` is the canonical read/write path.
- **Bulk metadata writes (M94).** `mp milestone bulk …` for multi-id
  set-priority / set-spec-status / depends-on.
- **Search v2 (M95).** `mp search <query>` returns fuzzy artifact hits with
  `suggested_action`s that map back to fragment commands.
- **Top-tab raul TUI (M91).** Horizontal lane tab bar; arrow / number / mouse
  navigation; never reverts to a sidebar.
- **Size-aware intake docs (M81).** Routing decision matrix teaches
  track vs milestone vs idea vs backlog first.

## Install

```bash
make install   # toolkit + OpenCode + Cursor + Pi (v1 trio)
mp doctor      # verify
```

## Compatibility & migration

- No back-compat shim for TOML plans.
- `MP_HOME` is now an override path only (templates/schemas embedded in binary).
- `--format raw` replaces `--format toml` for show/tracks debug output.

See CHANGELOG.md for the full set of changes and the rc.1–rc.4 history.
