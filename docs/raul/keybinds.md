# raul key bindings

This is the authoritative reference for raul's keys, in declaration order. Every
binding is also configurable — see [Customizing](#customizing) at the bottom.

## Navigation

| Action | Default keys |
|--------|--------------|
| Previous lane | `←`  `Shift+Tab` |
| Next lane | `→`  `l`  `Tab` |
| Jump to lane 1–7 | `1` `2` `3` `4` `5` `6` `7` |
| Focus content pane | `Enter` |
| Go back | `Esc` |
| Move up | `↑`  `k` |
| Move down | `↓`  `j` |
| Page up | `PageUp` |
| Page down | `PageDown` |

## Inside a milestone detail

These only do something when you have drilled into a milestone.

| Action | Default keys |
|--------|--------------|
| Next section | `]` |
| Previous section | `[` |
| Next list item (across sections) | `n` |
| Previous list item (across sections) | `p` |

## Lists and lanes

| Action | Default keys |
|--------|--------------|
| Select / drill in | `Enter` |
| Refresh (re-read from disk) | `Ctrl-R` |
| Toggle filter | `f` |
| Toggle hide-done | `h` |

> The Autopilot lane (lane 6, when visible) drives the `mp autopilot`
> workflow — see [Autopilot lane](#autopilot-lane) below.

## Filtering, sorting & search

These reshape the current list. The sort-rebind menu persists your choice per
lane through `mp config set sort.<lane> <key>`.

| Action | Default keys |
|--------|--------------|
| Open lifecycle filter modal | `Shift+F` |
| Apply Grooming preset (Milestones) | `g` |
| Open sort-rebind menu | `Shift+S` |
| Cycle sort key (inline, no menu) | `o` |
| Open search input | `/` |

While the sort-rebind menu is open, it is modal: `↑`/`↓` (or `k`/`j`) cycle the
sort key, `Enter` binds and closes, `Esc` cancels without binding.

## Autopilot lane

The **Autopilot** lane (lane 6, only visible when
`ui.show_autopilot_tab = true`) is the visual surface for the `mp autopilot`
workflow: it renders the drivable-milestone picker, the lifecycle graph, the
ordered queue, and the live agent output of an autopilot run. The lane is a
*view* — the actual control surface is the `mp autopilot` CLI
([`../mp/commands.md`](../mp/commands.md)):

```bash
mp autopilot start <id> [<id>…] [--dry-run]   # drive milestones through their lifecycle
mp autopilot status                            # queue, active milestone, stage, outcome
mp autopilot stop                              # gracefully stop the live run
mp autopilot output                            # bounded snapshot of the active pane
```

When `ui.show_autopilot_tab = false` (the default), the Autopilot lane is
hidden and **Settings is lane 6** instead of lane 7. Lane-number bindings
(`1`–`7`) skip the hidden lane; everything else in this file is lane-ordinal
agnostic. Use `mp config set ui.show_autopilot_tab true` to enable.

The lane's interactive keys (picker selection, dry-run preflight, start/stop)
are wired through the action set but not yet bound to the keyboard dispatcher;
use the CLI commands above to drive a run today.

## Annotations, approval, review

| Action | Default keys |
|--------|--------------|
| Create annotation | `Shift+A` |
| Resolve annotation | `r` |
| Reopen annotation | `Shift+R` |
| Toggle approval / request | `p` |
| Open review menu | `m` |
| Open Settings lane | `Ctrl+O` |

> The on-screen legend reflects the **content-canonical** meaning of `r`
> (resolve an open annotation). On a data lane without an open annotation,
> `Ctrl-R` is the refresh binding. The two never collide in practice —
> when an annotation thread is focused, `r` resolves; elsewhere, `Ctrl-R`
> refreshes.

## Global

| Action | Default keys |
|--------|--------------|
| Help (on-screen legend) | `?` |
| Quit | `q`  `Q` |

While the help overlay is open, **any** key closes it; `q`/`Q` also quits.

## Contextual overlaps (why the same key does two things)

A few keys are intentionally re-interpreted by what has focus. This is by design,
not a conflict:

- **`h`** — *hide-done* inside a list. (Previous-lane alias `h` was removed
  in the lane-rename refactor; `←` and `Shift+Tab` cover that role.)
- **`r`** — *resolve annotation* in an annotation thread; refresh on a data
  lane is `Ctrl-R`, never `r`.
- **`Tab`/`Shift+Tab`** — lane navigation (they used to toggle a focus state).

The on-screen legend (`?`) reflects the *content-canonical* meaning of each key;
contextual overrides are resolved by the focused pane first.

## Customizing

All bindings are configurable via the project config. Bind a single combo or a
list of combos. To set a binding:

```bash
mp config set keybinds.quit "q"
mp config set keybinds.up '["Up", "k"]'
```

Set a binding to an empty list `[]` to disable that action. Binding strings use
this grammar:

- Bare keys: `q`, `Up`, `Down`, `Enter`, `Esc`, `Tab`, `Backspace`, `space`,
  `pageup`, `pagedown`, `f1`…
- Modifiers (stack with `+`): `ctrl+`, `alt+`, `shift+`, `super+`, `hyper+`.
  Uppercase letters are treated as `shift+<letter>`.
- Examples: `ctrl+o`, `shift+tab`, `ctrl+shift+t`.

Malformed combos, wrong-typed values, or two actions bound to the same combo are
reported as warnings on startup; the affected field falls back to its default,
so a fat-fingered config never crashes the TUI.

See [`settings.md`](./settings.md) for the full UI preference surface.

## User-level `keybinds.toml` (override surface)

In addition to the project-config route above, raul reads
`~/.config/raul/keybinds.toml` (or `$XDG_CONFIG_HOME/raul/keybinds.toml`) at
startup. The file uses TOML sections per scope:

```toml
# Optional — only include overrides; defaults always live in code.
[global]
quit = "ctrl+x"
page_down = ["PageDown", "pagedown"]

[autopilot]
select = "f1"          # was Space
move_picker_up = "k"
```

Reload the file without restarting: on Unix, `kill -HUP <raul-pid>` requests
a reload (the signal handler only flips a flag — parse + swap run on the
next event-loop tick). On every platform the explicit reload action (see
the Settings lane) does the same swap.

Precedence: user-level `keybinds.toml` > hardcoded defaults. Reads never
write either source. The legacy mp-config `[keybinds]` JSON overlay was
removed entirely (no longer honored, no migration hint emitted) — move
any bindings you still want into `keybinds.toml`.
