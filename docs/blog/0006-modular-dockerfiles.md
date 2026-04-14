# amux 0.6: Modular Dockerfiles and per-session agents

One thing I kept running into while working with multiple agents on the same project: every time I wanted to try Codex or Gemini on something, I had to either reconfigure the whole repo or create a second clone just to get a different container image. The problem was that amux used a single `Dockerfile.dev` for everything — project tooling and agent tooling, all mixed together. Swapping agents meant rebuilding the whole image.

That changes in amux 0.6.

---

```sh
# install or upgrade
curl -s https://prettysmart.dev/install/amux.sh | sh
```

---

## Two Dockerfiles, two images

amux now separates container image setup into two layers:

**`Dockerfile.dev`** at your project root stays focused on your project — language runtimes, compilers, test runners, build tools. Nothing agent-specific. It produces `amux-{project}:latest`.

**`.amux/Dockerfile.{agent}`** extends that base and installs the AI agent on top. It produces `amux-{project}-{agent}:latest`. That's the image that actually runs your agent sessions.

The split is automatic. `amux init` writes both files, runs the audit to populate your project dependencies into `Dockerfile.dev`, then builds both images in sequence.

```
amux-myproject:latest           ← Dockerfile.dev (your project deps)
amux-myproject-claude:latest    ← .amux/Dockerfile.claude (claude + amux user)
amux-myproject-codex:latest     ← .amux/Dockerfile.codex  (codex + amux user)
```

Both files go in source control. Teammates get the same setup on `git clone`.

---

## Switch agents per session with `--agent`

The new `--agent` flag lets you pick an agent at launch time without touching any config:

```sh
amux chat --agent codex
amux implement 0049 --agent gemini
```

If the image for that agent doesn't exist yet, amux offers to build it on the spot — no manual setup needed. It builds the project base (if missing) first, then the agent image, then launches.

---

## Upgrading from the old layout

If you have an existing repo where `Dockerfile.dev` bundles agent tooling (the old single-file layout), `amux ready` detects it and walks you through a migration:

```
Detected legacy single-file Dockerfile.dev layout.
Would you like to migrate to the modular layout? (agent tools move to .amux/Dockerfile.{agent})

Migrating will:
  1. Recreate Dockerfile.dev with a minimal debian:bookworm-slim base
  2. Write .amux/Dockerfile.{agent} using the agent template
  3. Build both images
  4. Run the audit agent to restore project dependencies in Dockerfile.dev

[y/N]:
```

Accept, and amux handles it all — including re-running the audit to bring your project deps back into the new `Dockerfile.dev`. Decline, and your existing image keeps working until you're ready.

If you're not quite ready to migrate, you can also pass `--no-migrate` to `chat` or `implement` to skip the check and use the existing image for that session.

---

## Why this matters for `--refresh`

Before, running `amux ready --refresh` meant rebuilding an image that mixed your project and agent tooling together. Now, the audit agent only touches `Dockerfile.dev`. It doesn't know or care about the agent layer — that's managed separately by the template system.

This means after a `--refresh`, only your project deps change. The agent image inherits the rebuilt base automatically when you next run `--build`. Clean separation.

---

I've been running this locally for a while now and it makes agent experimentation a lot less friction. Give it a try and let me know how it goes!

---

Source and issues at [github.com/prettysmartdev/amux](https://github.com/prettysmartdev/amux). More at [prettysmart.dev](https://prettysmart.dev). Feedback, bug reports, and contributions are welcome.
