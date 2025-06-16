//! A simple Logger that appends log-level tags to a message and prints it with a specified prefix

use crate::utils::colors;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "[DEBUG]",
            LogLevel::Info => "[INFO]",
            LogLevel::Warn => "[WARN]",
            LogLevel::Error => "[ERROR]",
        }
    }

    pub fn color(&self) -> &'static str {
        match self {
            LogLevel::Debug => colors::BLUE,
            LogLevel::Info => colors::GREEN,
            LogLevel::Warn => colors::YELLOW,
            LogLevel::Error => colors::RED,
        }
    }
}

/// A simple Logger that appends log-level tags to a message and prints it with a specified prefix
/// Can optionally use colors for different log levels
#[derive(Debug, Clone)]
pub struct Logger {
    prefix: String,
    use_colors: bool,
    enabled: bool,
    min_level: LogLevel,
}

impl Logger {
    /// Creates a new Logger with the specified prefix and default log level (Debug)
    pub fn new(prefix: Option<&str>) -> Self {
        match prefix {
            Some(prefix) => Logger {
                prefix: format!("{prefix} "),
                use_colors: true,
                enabled: true,
                min_level: LogLevel::Debug,
            },
            None => Logger {
                prefix: String::new(),
                use_colors: true,
                enabled: true,
                min_level: LogLevel::Debug,
            },
        }
    }

    /// Creates a new Logger with the specified prefix and log level
    pub fn with_level(prefix: Option<&str>, level: LogLevel) -> Self {
        let mut logger = Self::new(prefix);
        logger.min_level = level;
        logger
    }

    /// Updates the logger prefix
    pub fn update_prefix(&mut self, prefix: &str) {
        self.prefix = format!("{prefix} ");
    }

    /// Sets the minimum log level
    pub fn set_level(&mut self, level: LogLevel) {
        self.min_level = level;
    }

    /// Gets the current minimum log level
    pub fn level(&self) -> LogLevel {
        self.min_level
    }

    /// Disables stdout output
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Enables stdout output
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Creates a new Logger with colors disabled (useful for testing)
    pub fn new_no_color(prefix: Option<&str>) -> Self {
        let mut logger = Self::new(prefix);
        logger.use_colors = false;
        logger
    }

    /// Formats a message with the appropriate color if colors are enabled
    fn format_msg(&self, level: LogLevel, msg: &str) -> String {
        if self.use_colors {
            format!(
                "{}{}{}{}{} {}",
                colors::BOLD,
                self.prefix,
                level.color(),
                level.as_str(),
                colors::RESET,
                msg
            )
        } else {
            format!("{}{} {}", self.prefix, level.as_str(), msg)
        }
    }

    /// Checks if a log level should be printed
    fn should_log(&self, level: LogLevel) -> bool {
        self.enabled && level >= self.min_level
    }

    /// Prints to `stdout` the `msg` and appends [INFO] to it
    pub fn info(&self, msg: &str) {
        if self.should_log(LogLevel::Info) {
            let msg = self.format_msg(LogLevel::Info, msg);
            println!("{msg}");
        }
    }

    /// Prints to `stderr` the `msg` and appends [ERROR] to it
    pub fn error(&self, msg: &str) {
        if self.should_log(LogLevel::Error) {
            let msg = self.format_msg(LogLevel::Error, msg);
            eprintln!("{msg}");
        }
    }

    /// Prints to `stdout` the `msg` and appends [WARN] to it
    pub fn warn(&self, msg: &str) {
        if self.should_log(LogLevel::Warn) {
            let msg = self.format_msg(LogLevel::Warn, msg);
            println!("{msg}");
        }
    }

    /// Prints to `stdout` the `msg` and appends [DEBUG] to it
    pub fn debug(&self, msg: &str) {
        if self.should_log(LogLevel::Debug) {
            let msg = self.format_msg(LogLevel::Debug, msg);
            println!("{msg}");
        }
    }

    /// Generic log method that accepts any log level
    pub fn log(&self, level: LogLevel, msg: &str) {
        if !self.should_log(level) {
            return;
        }

        let formatted_msg = self.format_msg(level, msg);
        match level {
            LogLevel::Error => eprintln!("{formatted_msg}"),
            _ => println!("{formatted_msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A helper trait to expose the format_msg method for testing
    trait LoggerTest {
        fn test_format_msg(&self, level: LogLevel, msg: &str) -> String;
    }

    impl LoggerTest for Logger {
        fn test_format_msg(&self, level: LogLevel, msg: &str) -> String {
            self.format_msg(level, msg)
        }
    }

    #[test]
    fn logger_message_info_without_prefix_is_composed_correctly() {
        let logger = Logger::new_no_color(None);

        let logged_msg = logger.test_format_msg(LogLevel::Info, "hello world");
        assert_eq!(logged_msg, "[INFO] hello world");
    }

    #[test]
    fn logger_message_info_with_prefix_is_composed_correctly() {
        let logger = Logger::new_no_color(Some("[CLIENT]"));

        let logged_msg = logger.test_format_msg(LogLevel::Info, "hello world");
        assert_eq!(logged_msg, "[CLIENT] [INFO] hello world");
    }

    #[test]
    fn test_log_levels() {
        // Create logger with Info level - Debug messages won't be shown
        let mut logger = Logger::with_level(Some("APP"), LogLevel::Info);

        logger.debug("This won't be printed"); // Below Info level
        logger.info("This will be printed"); // Info level
        logger.warn("This will be printed"); // Above Info level
        logger.error("This will be printed"); // Above Info level

        // Change log level at runtime
        logger.set_level(LogLevel::Error);
        logger.info("This won't be printed now"); // Below Error level
        logger.error("This will still be printed"); // Error level
    }

    #[test]
    fn test_all_log_levels_format_correctly() {
        let logger = Logger::new_no_color(Some("[TEST]"));

        assert_eq!(
            logger.test_format_msg(LogLevel::Debug, "debug message"),
            "[TEST] [DEBUG] debug message"
        );
        assert_eq!(
            logger.test_format_msg(LogLevel::Info, "info message"),
            "[TEST] [INFO] info message"
        );
        assert_eq!(
            logger.test_format_msg(LogLevel::Warn, "warn message"),
            "[TEST] [WARN] warn message"
        );
        assert_eq!(
            logger.test_format_msg(LogLevel::Error, "error message"),
            "[TEST] [ERROR] error message"
        );
    }

    #[test]
    fn test_should_log_respects_minimum_level() {
        let logger = Logger::with_level(Some("TEST"), LogLevel::Warn);

        // Should not log Debug and Info (below Warn)
        assert!(!logger.should_log(LogLevel::Debug));
        assert!(!logger.should_log(LogLevel::Info));

        // Should log Warn and Error (at or above Warn)
        assert!(logger.should_log(LogLevel::Warn));
        assert!(logger.should_log(LogLevel::Error));
    }
}
