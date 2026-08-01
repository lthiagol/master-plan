# raul settings and preferences

`raul` is read-only with respect to the plan, but it owns a small set of UI
preferences. Because `raul` never writes plan files, **every preference is stored
in the project config (`config.toml`) under `[ui]` and `[keybinds]`**, read via
`mp config show`, and written via `mp config set` (including from the Settings
lane).

## The Settings lane

Open it with `Ctrl+O` or by jumping to lane `5`. There you can toggle and save:

- **Color** — `on` / `off`
- **Icons** — `unicode` / `ascii` / `none`
- **Theme** — see below
- **Hide done** — hide completed items from lists

The footer shows `[Save (s)]` (with a `*` when you have unsaved staged edits) and
`[Cancel (Esc)]`. Press `s` to persist; `Esc` to discard. Saving calls
`mp config set` under the hood.

## UI preferences (`[ui]`)

| Key | Values | Default | Effect |
|-----|--------|---------|--------|
| `ui.color` | `true` / `false` | `true` | Enable ANSI color output |
| `ui.icons` | `unicode` / `ascii` / `none` | `unicode` | Status icons (`●◐✕○`) vs ASCII (`[x][~][!]`) vs none |
| `ui.theme` | a theme name (below) | `mocha` | Color palette |
| `ui.hide_done` | `true` / `false` | `false` | Hide completed items in lists |

Set from the shell:

```bash
mp config set ui.color true
mp config set ui.icons ascii
mp config set ui.theme dracula
mp config set ui.hide_done false
```

The `--color` CLI flag overrides `ui.color` for a single run.

## Themes

`raul` ships these palettes:

| Theme | Style |
|-------|-------|
| `latte` | Catppuccin light |
| `frappe` | Catppuccin (mid) |
| `macchiato` | Catppuccin (dark) |
| **`mocha`** | Catppuccin dark — the default |
| `dracula` | Dracula |
| `monochrome` | No color accents |

An unknown theme name falls back to `mocha`.

## Key bindings (`[keybinds]`)

Every navigation key is configurable. See [`keybinds.md`](./keybinds.md) for the
full binding table and the customization grammar. Summary:

```bash
mp config set keybinds.quit "q"
mp config set keybinds.next_lane '["Right", "l", "Tab"]'
```

## Review-side integrations (`[review]`)

These are project-wide flags that `mp` uses for the hunk-compatible findings
export. `raul` reads `review.hunk` only to show a "hunk export: on" indicator on
the milestone detail view.

| Key | Default | Effect |
|-----|---------|--------|
| `review.hunk` | `false` | Enable `mp reviews hunk <id>` to emit hunk-compatible JSON |
| `review.hunk_author` | `mp` | Author string baked into exported annotations |

```bash
mp config set review.hunk true
mp config set review.hunk_author "reviewer:alice"
```

## Validating config

```bash
mp config validate
```

Emits `{ ok, errors[], warnings[] }`. raul also surfaces binding/config
diagnostics as startup warnings and falls back to defaults, so a bad value never
prevents launch — but `validate` tells you exactly what is wrong.
