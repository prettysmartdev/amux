use std::io::Write;
use tokio::sync::mpsc::UnboundedSender;

/// Routes command output to either stdout (command mode) or a TUI channel (interactive mode).
///
/// This abstraction lets every command function work identically in both execution
/// contexts without duplicating logic.
#[derive(Clone)]
pub enum OutputSink {
    Stdout,
    Channel(UnboundedSender<String>),
    /// Discards all output. Used when suppressing human-readable output (e.g. --json mode).
    Null,
    /// Test-only variant: behaves like `Stdout` (supports_color = true, interactive paths
    /// are exercised) but captures all output to a channel and serves mock user input from
    /// a pre-loaded queue instead of reading from stdin.
    #[cfg(test)]
    MockInput {
        tx: UnboundedSender<String>,
        input: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    },
}

impl OutputSink {
    /// Construct a `MockInput` sink for tests.
    /// `inputs` is the ordered list of lines that `read_line()` will return (one per call).
    #[cfg(test)]
    pub fn mock_input(tx: UnboundedSender<String>, inputs: Vec<impl Into<String>>) -> Self {
        use std::collections::VecDeque;
        OutputSink::MockInput {
            tx,
            input: std::sync::Arc::new(std::sync::Mutex::new(
                inputs.into_iter().map(Into::into).collect::<VecDeque<_>>(),
            )),
        }
    }

    /// Returns true when the sink writes directly to a terminal (stdout),
    /// enabling ANSI colour output. `MockInput` also returns true so interactive
    /// code paths are exercised in tests. Returns false for TUI channel sinks.
    pub fn supports_color(&self) -> bool {
        match self {
            OutputSink::Stdout => true,
            OutputSink::Channel(_) => false,
            OutputSink::Null => false,
            #[cfg(test)]
            OutputSink::MockInput { .. } => true,
        }
    }

    pub fn println(&self, s: impl Into<String>) {
        match self {
            OutputSink::Stdout => println!("{}", s.into()),
            OutputSink::Channel(tx) => {
                let _ = tx.send(s.into());
            }
            OutputSink::Null => {}
            #[cfg(test)]
            OutputSink::MockInput { tx, .. } => {
                let _ = tx.send(s.into());
            }
        }
    }

    pub fn print(&self, s: impl Into<String>) {
        match self {
            OutputSink::Stdout => {
                print!("{}", s.into());
                let _ = std::io::stdout().flush();
            }
            OutputSink::Channel(tx) => {
                let _ = tx.send(s.into());
            }
            OutputSink::Null => {}
            #[cfg(test)]
            OutputSink::MockInput { tx, .. } => {
                let _ = tx.send(s.into());
            }
        }
    }

    /// Send a line, returning `true` on success.
    ///
    /// For `Stdout`, always succeeds. For `Channel`, returns `false` when the
    /// receiver has been dropped (e.g. the TUI tab was replaced by a new command).
    /// Callers can use this to detect channel closure and terminate watch loops.
    pub fn try_println(&self, s: impl Into<String>) -> bool {
        match self {
            OutputSink::Stdout => {
                println!("{}", s.into());
                true
            }
            OutputSink::Channel(tx) => tx.send(s.into()).is_ok(),
            OutputSink::Null => true,
            #[cfg(test)]
            OutputSink::MockInput { tx, .. } => tx.send(s.into()).is_ok(),
        }
    }

    /// Read one line of user input.
    ///
    /// - `Stdout`: reads from stdin (interactive terminal).
    /// - `Channel`/`Null`: returns an empty string (non-interactive; callers treat this as
    ///   a default/no answer).
    /// - `MockInput`: pops the next queued response (test-only).
    pub fn read_line(&self) -> String {
        match self {
            OutputSink::Stdout => {
                use std::io::BufRead;
                std::io::stdin()
                    .lock()
                    .lines()
                    .next()
                    .unwrap_or(Ok(String::new()))
                    .unwrap_or_default()
            }
            OutputSink::Channel(_) | OutputSink::Null => String::new(),
            #[cfg(test)]
            OutputSink::MockInput { input, .. } => {
                input.lock().unwrap().pop_front().unwrap_or_default()
            }
        }
    }

    /// Print a `[y/N]` prompt and return `true` if the user answers yes.
    pub fn ask_yes_no(&self, prompt: &str) -> bool {
        self.print(format!("{} [y/N]: ", prompt));
        let answer = self.read_line();
        matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn channel_sink_delivers_messages() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        sink.println("hello");
        sink.println("world");
        assert_eq!(rx.try_recv().unwrap(), "hello");
        assert_eq!(rx.try_recv().unwrap(), "world");
    }

    #[test]
    fn try_println_returns_true_for_open_channel() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        assert!(sink.try_println("msg"));
        assert_eq!(rx.try_recv().unwrap(), "msg");
    }

    #[test]
    fn try_println_returns_false_for_dropped_receiver() {
        let (tx, rx) = unbounded_channel::<String>();
        drop(rx);
        let sink = OutputSink::Channel(tx);
        assert!(!sink.try_println("msg"));
    }

    // ── MockInput ──────────────────────────────────────────────────────────────

    #[test]
    fn mock_input_supports_color_returns_true() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);
        assert!(sink.supports_color(), "MockInput should behave like Stdout for supports_color");
    }

    #[test]
    fn mock_input_captures_output_to_channel() {
        let (tx, mut rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec![] as Vec<String>);
        sink.println("hello");
        sink.print("world");
        let msgs: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        assert_eq!(msgs, vec!["hello", "world"]);
    }

    #[test]
    fn mock_input_read_line_pops_queue_in_order() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["first", "second", "third"]);
        assert_eq!(sink.read_line(), "first");
        assert_eq!(sink.read_line(), "second");
        assert_eq!(sink.read_line(), "third");
        assert_eq!(sink.read_line(), "", "empty string after queue exhausted");
    }

    #[test]
    fn mock_input_ask_yes_no_returns_true_for_y() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["y"]);
        assert!(sink.ask_yes_no("Continue?"));
    }

    #[test]
    fn mock_input_ask_yes_no_returns_false_for_n() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::mock_input(tx, vec!["n"]);
        assert!(!sink.ask_yes_no("Continue?"));
    }

    #[test]
    fn channel_read_line_returns_empty() {
        let (tx, _rx) = unbounded_channel();
        let sink = OutputSink::Channel(tx);
        assert_eq!(sink.read_line(), "");
    }
}
