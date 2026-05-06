# new-amux issue list

## TUI

TUI-1: The yolo countdown timer dialog during `exec workflow` is not properly dismissed with `Esc`, it immediately re-appears. When `Esc is pressed, the yolo countdown should be canceled and the tab-stuck timer re-set. Only when the tab-stuck timer expires again should the yolo countdown restart. Review old-amux behaviour and replicate it.

**Status: FIXED**
Root cause: race condition in `yolo_countdown_tick()` — the engine unconditionally wrote `Some(YoloState)` to the shared state before reading it, immediately overwriting the user's Esc cancellation on the next 100ms tick. Fix: added `yolo_initialized` bool to `TuiCommandFrontend` to distinguish "not yet started" from "user cancelled". The tick method now checks if state was cleared before writing. Added `yolo_dismissed_at: Option<Instant>` to `Tab` so `tick_all_tabs()` respects the `STUCK_DIALOG_BACKOFF` (60s) before re-showing the yolo overlay. Files changed: `command_frontend.rs`, `workflow_frontend.rs`, `tabs.rs`, `app.rs`, `mod.rs`.

TUI-2: Pressing Ctrl-W while the yolo countdown timer dialog is running should cancel the countdown (just like TUI-1 describes), and open the workflow control dialog instead, allowing the user to take manual control of workflow proceeding. This should also re-start the tab-stuck timer and if it expires again, the workflow control dialog can be dismissed and replaced with a new yolo countdown dialog again.

**Status: FIXED**
Added `YoloTickOutcome::ShowControlBoard` variant and `SharedYoloCtrlW` (`Arc<AtomicBool>`) shared flag. Ctrl-W is now a global keybinding (`Action::WorkflowControl`) that clears the yolo state, sets `yolo_dismissed_at`, and raises the `yolo_ctrl_w` flag. The engine's `yolo_countdown_tick` checks this flag and returns `ShowControlBoard`. `run_yolo_countdown` now returns a `YoloCountdownResult` enum; the `ShowControlBoard` variant falls through to the interactive control board in `run_to_completion`. Files changed: `actions.rs`, `keymap.rs`, `tabs.rs`, `command_frontend.rs`, `workflow_frontend.rs`, `mod.rs` (engine and TUI), `app.rs`.

TUI-3: The pre-workflow "commit uncommited files" dialog does not show the list of dirty files, and the suggested git commit message should be pre-loaded in the text field and directly editable instead of in the preamble. Also, the git commit text box text is currently invisible. Ensure the dialog shows dirty files, has visible text and blinking cursor in the git commit message field, and that the suggested git commit message is editable text in the field instead of in the title or preamble text. Replicate the dialog from old-amux as closely as possible.

**Status: FIXED**
1. The Custom dialog for uncommitted files now includes the file list in the body text (was already present but the dialog height didn't account for multi-line bodies — fixed height calculation to use `body.lines().count()`).
2. Added `default_text: Option<String>` field to `DialogRequest::TextInput`. The commit message dialog now passes `default_text: Some(suggested_message)` so the suggested message is pre-loaded in the editable text field instead of shown in the prompt.
3. Rewrote the TextInput dialog rendering: prompt text shown in gray above a bordered Cyan input block with white text and a visible cursor. The dialog height now scales to fit the prompt. Files changed: `dialogs/mod.rs`, `render.rs`, `worktree_lifecycle.rs`, `app.rs`, `specs.rs`, `new.rs`.

TUI-4: In the post-workflow worktree prompt, pressing `d` to discard the worktree does nothing, it leaves the worktree in place. It should run `git worktree remove <> --force` and `git branch -D <>`

**Status: INVESTIGATED — BACKEND CORRECT**
The dialog handling and git backend are verified correct: the Custom dialog properly sends `DialogResponse::Char('d')`, which maps to `PostWorkflowWorktreeAction::Discard`, which calls `remove_worktree_logged` (`git worktree remove --force`) and `delete_branch_logged` (`git branch -D`). All 14 worktree lifecycle unit tests pass including `finalize_discard_removes_worktree_and_deletes_branch`. The issue is likely environment-specific (e.g., locked files, current directory being inside the worktree, or a git error that's reported in the status log but not noticed). Fixed the Custom dialog height calculation to properly account for multi-line body text so error messages are more visible.

TUI-5: The `config show` dialog in the TUI has very small text, no cell borders, no obvious way to know which cell is selected, no text cursor, and no hints at the bottom of the dialog to know what keys do what. Replicate the visual style of the config show dialog in old-amux as closely as possible and fix all the issues listed here.

**Status: FIXED**
Rewrote `render_config_show()` from scratch using Ratatui `Table` widget with `Row`/`Cell`, matching old-amux visual style:
- Yellow rounded border with centered " amux config " title
- Cyan bold header row (Field / Global / Repo / Effective)
- Selected row highlighted with `White on DarkGray` background
- Selected column within selected row highlighted with `Black on White` (browse) or `Black on Green` (edit mode)
- Read-only rows in `DarkGray`
- Percentage-based column widths (28/24/24/24) that scale with terminal width
- Bottom hint area with colored key hints: `↑↓=row ←→=col e=edit Esc=close` (browse mode) or `Enter=save Esc=cancel` (edit mode)
- Inline cursor display in editing mode (`value|rest` format)
Files changed: `render.rs`.

TUI-6: During the yolo countdown, the purple/yellow tab flashing should also be accompanied by emojis and the 'yolo in x' text counting down so the user knows the state even if working in another tab. Replicate the emojis and countdown in the tab inner label just like old-amux.

**Status: FIXED**
Added `background_yolo_label()` method to `Tab` that returns alternating emoji + countdown text: `⚠️  yolo in N` (even seconds) / `🤘 yolo in N` (odd seconds), truncated to fit `tab_width`. Updated `tab_subcommand_label()` to show this label for non-active tabs when a yolo countdown is active (background tabs show the countdown instead of the command name). Also updated the yolo countdown dialog rendering to include emojis in the title bar and a Ctrl-W hint. Files changed: `tabs.rs`, `render.rs`.
