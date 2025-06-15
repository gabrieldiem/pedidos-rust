//! A simple Logger that appends log-level tags to a message and prints it with a specified prefix

use crate::utils::colors;

/// A simple Logger that appends log-level tags to a message and prints it with a specified prefix
/// Can optionally use colors for different log levels
#[derive(Clone, Debug)]
pub struct Logger {
    prefix: String,
    use_colors: bool,
}

impl Logger {
    /// Creates a new Logger with the specified prefix
    pub fn new(prefix: Option<&str>) -> Self {
        match prefix {
            Some(prefix) => Logger {
                prefix: format!("{prefix} "),
                use_colors: true,
            },
            None => Logger {
                prefix: String::new(),
                use_colors: true,
            },
        }
    }

    /// Updates the logger prefix
    pub fn update_prefix(&mut self, prefix: &str) {
        self.prefix = format!("{prefix} ");
    }

    /// Creates a new Logger with colors disabled (useful for testing)
    pub fn new_no_color(prefix: Option<&str>) -> Self {
        let mut logger = Self::new(prefix);
        logger.use_colors = false;
        logger
    }

    /// Formats a message with the appropriate color if colors are enabled
    fn format_msg(&self, level: &str, color: &str, msg: &str) -> String {
        if self.use_colors {
            format!(
                "{}{}{}{}{} {}",
                colors::BOLD,
                self.prefix,
                color,
                level,
                colors::RESET,
                msg
            )
        } else {
            format!("{}{} {}", self.prefix, level, msg)
        }
    }

    /// Prints to `stdout` the `msg` and appends [INFO] to it
    pub fn info(&self, msg: &str) {
        let msg = self.format_msg("[INFO]", colors::GREEN, msg);
        println!("{msg}");
    }

    /// Prints to `stderr` the `msg` and appends [ERROR] to it
    pub fn error(&self, msg: &str) {
        let msg = self.format_msg("[ERROR]", colors::RED, msg);
        eprintln!("{msg}");
    }

    /// Prints to `stdout` the `msg` and appends [WARN] to it
    pub fn warn(&self, msg: &str) {
        let msg = self.format_msg("[WARN]", colors::YELLOW, msg);
        println!("{msg}");
    }

    /// Prints to `stdout` the `msg` and appends [DEBUG] to it
    pub fn debug(&self, msg: &str) {
        let msg = self.format_msg("[DEBUG]", colors::BLUE, msg);
        println!("{msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A helper trait to expose the format_msg method for testing
    trait LoggerTest {
        fn test_format_msg(&self, level: &str, color: &str, msg: &str) -> String;
    }

    impl LoggerTest for Logger {
        fn test_format_msg(&self, level: &str, color: &str, msg: &str) -> String {
            self.format_msg(level, color, msg)
        }
    }

    #[test]
    fn logger_message_info_without_prefix_is_composed_correctly() {
        let logger = Logger::new_no_color(None);

        let logged_msg = logger.test_format_msg("[INFO]", colors::GREEN, "hello world");
        assert_eq!(logged_msg, "[INFO] hello world");
    }

    #[test]
    fn logger_message_info_with_prefix_is_composed_correctly() {
        let logger = Logger::new_no_color(Some("[CLIENT]"));

        let logged_msg = logger.test_format_msg("[INFO]", colors::GREEN, "hello world");
        assert_eq!(logged_msg, "[CLIENT] [INFO] hello world");
    }
}
