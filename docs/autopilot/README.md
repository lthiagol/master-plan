# Autopilot

The `mp autopilot` orchestration surface. Each autopilot session is a
self-contained **orchestrator + runner + reviewer** workflow, tracked in
`<plan_dir>/autopilot/<id>/session.json` so it can be archived, diffed, and
recovered in isolation.

## Reference

- [`session-format.md`](./session-format.md) — the JSON schema, the topology
  fields, and the runtime state machine
- [`migration.md`](./migration.md) — schema-version upgrades and how to migrate
  older sessions