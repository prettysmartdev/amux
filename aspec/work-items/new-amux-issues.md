# New-amux issues

## TUI

TUI-1: The bottom text does not show 'Using worktree...' when `exec workflow` runs in a worktree, it shows the CWD. Fix it.

TUI-2: When the yolo dialog is shown in a workflow, it says you can press Ctrl-W for the workflow control board, but pressing Ctrl-W just dismisses the yolo dialog and no workflow control dialog shows up.

TUI-3: The container window PTY still doesn't let me scroll all the way to the bottom AND still limits 50 lines of scrollback even when there are 1000+ available. Fix scrolling properly to work like old-amux.

TUI-4: After a workflow completes while running in a worktree, the optiones include 'merge into main branch' which may be misleading if the branch being merged into is not `main`. Change to `merge into current branch' or fetch the actual branch name if you can, for clarity

TUI-5: The 'Commit before merge?" dialog cuts off the text, shows no hints, and accepts no input except Esc. Fix it.

## Engines

ENG-1: While `exec workflow` does detect if there is an active worktree and asks to resume using it or re-create it, it does not detect and existing workflow state file and ask if the workflow should be resumed or deleted and started fresh. Ensure it asks about workflow resumption AND worktree reuse/recreate when each thing is found on disk, respectively.

ENG-2: When running a workflow with --yolo, the yolo dialog shows up in the TUI but never starts counting down; it sticks at 60 and nothing advances. Ensure the countdown and auto-advance work properly.
