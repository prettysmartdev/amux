# amux 0.7: Multi-agent workflows

Different tasks benefit from different agents. Claude is strong at open-ended reasoning and planning; Codex is fast and precise at targeted edits; Gemini brings a different perspective. Until now, a workflow had to pick one agent and stick with it — if you wanted to plan with Claude and implement with Codex, you ran two separate sessions by hand.

amux 0.7 lets you assign a different agent to each workflow step.

---

```sh
# install or upgrade
curl -s https://prettysmart.dev/install/amux.sh | sh
```

---

## Per-step agents in workflow files

Add an `Agent:` field to any step header to specify which agent runs that step:

```markdown
# Feature Workflow

## Step: plan
Prompt: Read the work item and produce a detailed implementation plan.

{{work_item_content}}

## Step: implement
Depends-on: plan
Agent: codex
Prompt: Implement work item {{work_item_number}} according to the plan.

## Step: review
Depends-on: implement
Agent: claude
Prompt: Review the changes for correctness, edge cases, and style.

## Step: docs
Depends-on: implement
Prompt: Write user-facing documentation for the changes.
```

`Agent:` is optional. Steps without it use the workflow default — the agent from your repo config, or whatever you pass with `--agent` at the command line. The `--agent` flag sets the default for unspecified steps; it does not override steps that explicitly name one.

---

## Pre-flight check

Before the first step runs, amux checks that every required agent image exists. If an image is missing, it asks:

```
Agent 'codex' has no Dockerfile. Download and build it? [y/N]:
```

Accept, and amux fetches the agent template, builds the project base image (if needed), then builds the agent image — all before a single step starts. Decline, and amux asks if you want to fall back to the default agent for those steps instead. No mid-workflow surprises.

---

## "Same container" across agent boundaries

The workflow control board's **↓** action ("next step: same container") lets you send the next prompt to the already-running container — handy when the container has already compiled the project or pulled down dependencies. That option is unavailable when the next step needs a different agent, since you can't run Codex inside a Claude container:

```
Next step uses agent 'codex'; cannot reuse current 'claude' container.
```

The **→** action (new container) always works.

---

## `ready --build` with multiple agents

If you have Dockerfiles for more than one agent in `.amux/`, `amux ready --build` now asks which to build rather than rebuilding everything silently:

```
Found 2 agent Dockerfiles:
  claude  (default)
  codex   (extra)

Build all agent images, or only the default (claude)? [all/default]:
```

And `--no-cache` now consistently applies to every image in the build sequence — project base and all agent images.

---

## Build output

Each build is now framed with visible start and end markers, so you can follow a multi-image sequence without losing track of where you are in the output:

```
══════════════════════════════════════════════════
  Building agent image: amux-myproject-codex:latest
══════════════════════════════════════════════════
```

This applies to all build paths: `amux init`, `ready --build`, `ready --refresh`, and on-demand builds triggered by `--agent` with a missing image.

---

Feedback, bugs, and contributions welcome at [github.com/prettysmartdev/amux](https://github.com/prettysmartdev/amux). More at [prettysmart.dev](https://prettysmart.dev).
