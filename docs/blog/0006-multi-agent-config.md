# amux 0.6: Multi-agent workflows, config commands, and true background yolo

I've been running amux in aspec-free repos and wishing I could wire different agents to different workflow steps for a while. Both of those are now done. v0.6 is the release I've been looking forward to writing.

---

```sh
# install or upgrade
curl -s https://prettysmart.dev/install/amux.sh | sh
```

---

## One workflow, multiple agents

The big one: each step in a workflow can now name a specific agent.

```markdown
## Step: plan
Prompt: Read the work item and produce an implementation plan.

## Step: implement
Depends-on: plan
Agent: codex
Prompt: Implement the plan from the previous step.

## Step: review
Depends-on: implement
Agent: claude
Prompt: Review the implementation for correctness and style.
```

Steps without an `Agent:` field use whatever agent is configured for the repo (or whatever you pass via `--agent`). Before the workflow starts, amux checks that every required image exists and walks you through building anything that's missing — you're not dropped mid-workflow with a missing image error.

You can also pass `--agent` on any single `chat` or `implement` invocation to override the configured agent for that session:

```sh
amux chat --agent codex
amux implement 0047 --agent gemini
```

---

## amux config

Until now, configuring amux meant opening `~/.amux/config.json` or `.amux/.amux.json` by hand. That was fine for initial setup but annoying for one-off changes and almost impossible to debug when two files disagreed on a value.

```sh
amux config show
```

```
Field                       Global              Repo              Effective          Override
──────────────────────────  ──────────────────  ────────────────  ─────────────────  ────────
default_agent               claude (built-in)   N/A               claude             —
runtime                     docker (built-in)   N/A               docker             —
terminal_scrollback_lines   10000 (built-in)    5000              5000               yes
agent                       N/A                 codex             codex              —
work_items.dir              N/A                 docs/work-items   docs/work-items    —
```

`config show` renders every field — even unset ones — with built-in defaults and an override indicator when the repo value shadows the global. `config get <field>` drills into one field. `config set` writes and warns you when the new value will be silently overridden.

```sh
amux config set agent codex
amux config set --global default_agent gemini
amux config set work_items.dir docs/work-items
```

---

## amux works in any repo now

`specs new` and `implement` were hardcoded to look in `aspec/work-items/`. If your repo doesn't use that structure, you were stuck.

```sh
amux config set work_items.dir docs/tickets
amux config set work_items.template docs/tickets/0000-template.md
```

Once set, every amux command that touches work items uses the configured paths. If you haven't set a template, `specs new` scans the directory for `*template.md` files and offers to save the match. `amux init` also prompts for a custom work items directory if `aspec/` doesn't exist, so the first-run experience works cleanly in non-aspec repos.

---

## Background yolo that actually runs in the background

The v0.5 yolo countdown dialog only fired on the active tab. If a workflow step finished on a background tab, nothing happened until you switched to it. That made running workflows in multiple tabs awkward — you had to keep checking.

Now the tab bar shows a live countdown for any background tab that's waiting:

```
┌─ Tab 1: myproject ─────────┬─ Tab 2 🤘 yolo in 38 ──────┐
│  chat                        │                              │
└──────────────────────────────┴──────────────────────────────┘
```

The tab alternates between yellow and purple every second. When the timer hits zero, the workflow advances — no switching required. If you do switch to the tab mid-countdown, the dialog opens showing the remaining time (not restarted from 60). Press `Ctrl+A` or `Ctrl+D` to leave the dialog and keep the countdown going in the background.

One more small thing: the stuck timer and yolo dialog no longer fire while you're actively scrolling through output on the active tab. Both the container and you have to be idle for 10 seconds before the countdown starts.

---

Source and issues at [github.com/prettysmartdev/amux](https://github.com/prettysmartdev/amux). More at [prettysmart.dev](https://prettysmart.dev). Feedback, bug reports, and contributions are welcome.
