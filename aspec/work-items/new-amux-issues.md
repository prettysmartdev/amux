# new-amux issue list

## TUI

TUI-1: When `exec workflow` is run, the workflow state strip does not immediately show up (might be covered by the execution window and/or container window?) Ensure it shows up immediately when a workflow becomes active and that when a workflow ends and the user runs a different command, the session's active workflow is wiped and the state strip is reset (meaning it dissapears if there's no active workflow for the new command, or it is rendered from scratch if the user runs another workflow). The state strip shows up AFTER the first step complets, which means something is not happening in the correct order.

TUI-2: Somehow, after 5 attempts to fix it, CONTAINER STATS ARE STILL NOT SHOWING IN THE TOP RIGHT CORNER OF THE CONTAINER WINDOW TITLE. The container NAME is now showing which is a small improvement, but the CPU and memory stats are not showing. It's unacceptable that this has been broken for this long despite trying to get it fixed so many times. FIGURE IT OUT AND FIX IT. FOR DOCKER AND APPLE. 

TUI-3: Container window PTY scrollback should not be limited to 50 lines, it should default to 5000 lines, and the repo and/or global config should properly allow it to be configured to the user's preference. Ensure scrollback works identically to old-amux and that the repo config overrides the global config if they are both set.

TUI-4 The dirty files in the git wortree are STILL not showing in the pre-workflow git worktree prep dialog. Ensure the full list of files is shown IN THE DIALOG in addition to the execution window's output. Also, add some padding below the git commit message text field.

TUI-5: Add padding below the text field in the new tab dialog.

TUI-6: When a workflow is active in the current tab and running in a worktree, show `Using worktree: <path>` at the bottom below the command text box instead of the CWD. `Using worktree` should be blue and the worktree path itself should be grey. Copy old-amux for this. Ensure the use of a worktree is tracked in the Tab's `Session` along with the active `WorkflowState`.

TUI-7: Ctrl-W still does not dismiss the yolo countdown dialog and show the workflow control dialog. Ensure that Ctrl-W works properly.

TUI-8: After the yolo countdown reaches 0 and the next container in the workflow is launched, the yolo dialog should dissapear. It currently stays visible even thought the countdown is 0 and the next step is running. The same is true for a tab running a yolo countdown in the background. After the countdown expires, the tab label and color should reset and reflect the current status of the new running step automatically, even if the user doesn't switch back to that tab.

TUI-9: Sometimes scrolling in the container PTY gets messed up and I can't scroll all the way to the bottom of Claude's TUI after I scrolled to the top of the available scrollback. Ensure scrolling in both directions works properly (tied to TUI-3).

TUI-10: When a workflow is running in a tab, add the step name to the inner text label of the tab itself, like `exec workflow: implement (1/5)`. Ensure tab sizes grow appropriately for the size of their inner label unless they need to be truncated when there's too many tabs for the window width. Ensure the tab inner label updates each time the workflow status changes.

## Command Layer

COM-1: The `status` command is not showing any running agent containers even when one is running. Fix the status command, ensure it shows everything exactly the same as old-amux, that it works with both Docker and Apple, and that it includes the tab number for any container that is running in the same TUI as `status` is being run in. Ensure it all renders correctly in tables in the TUI and CLI frontends, and that `--status` properly keeps things updating in both frontends.

## Engine Layer

ENG-1: Running a workflow in new-amux does not currently persist workflow state in $GITROOT/.amux/workflows/... review what old-amux did and ensure that workflow persistence works AND is updated after every step AND that workflows can be resumed if there is unfinished workflow state on disk AND that all of the dialogs to support that are properly wired into the TUI and CLI frontends. This should behave identically to old-amux. Ensure that if a step was marked as 'running', that the user is given the choice to restart that step or move to the next one when resuming a workflow.

ENG-2: A frontend must be able to report into the WorkflowEngine that an agent container for the current step is stuck, which must cause the WorkflowEngine to either 1) trigger the workflow control board to be shown by the frontend or 2) Trigger the yolo countdown automatically. Neither of those things happens right now. Ensure that a stuck container (as detectd by the TUI, for example) causes SOMETHING to happen by reporting into the workflowengine and having the engine make the right choice for what the frontend is supposed to do.

ENG-3: Work item section template insertion does not seem to be working. Ensure that work item sections are parsed and any workflow prompts with work item section template markers get the correct template section's text inserted. Do it exactly like old-amux did. Ensure all different types of workflow step prompt template insertion are working just like old-amux.
