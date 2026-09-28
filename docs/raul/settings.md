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

An unknown theme name falls back to `mocha`.

> `monochrome` exists internally as the no-accent palette used when
> `ui.color` is disabled, but it is **not** a user-selectable theme —
> `ui.theme = monochrome` falls back to `mocha` like any other unknown
> value. Disable color via `ui.color = false` instead.

### Color roles

Each palette maps nine semantic roles to concrete colors. Renderers
consume the role, never a literal color, so switching `ui.theme`
recolors the whole surface.

| Role | Used for |
|------|----------|
| `accent` | Headers, active items, the in-progress lifecycle color |
| `success` | Done / verified / passed |
| `warn` | Ready / pending, cancelled milestones |
| `danger` | Blocked / failure |
| `dim` | Secondary text, tree connectors, inactive tabs |
| `foreground` | Primary body text, and the text drawn on `surface_1` / `surface_2` |
| `focus_ring` | Where the cursor is: the focused tab's fill, the selected row's border and marker glyph, the selected board box, the focused Path node |
| `surface_1` | The panel a surface floats on: overlay / modal backdrops and the header + footer chrome bands |
| `surface_2` | One layer up, inside a surface: the selected or hovered list row, the selected board box, the selected row in a modal or picker, and detail-view AC rows |

The two surface roles are deliberately quiet near-background tones, not
saturated fills: `surface_2` sits on `surface_1`, and `foreground` is the
readable text color on both. With `ui.color = false` every color role
collapses to the terminal default and layering falls back to bold
selection plus reversed cursor text.

## Key bindings

Every navigation key is configurable via the user-level
`~/.config/raul/keybinds.toml` (see [`keybinds.md`](./keybinds.md) for the
full binding table and the customization grammar). The legacy project-config
`[keybinds]` JSON overlay was removed; use `mp config set` only for non-keybind
preferences.

Summary:

```bash
mp config set ui.color false
mp config set ui.icons ascii
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
