use diagnostic_msgs::msg::{DiagnosticStatus, KeyValue};
use std::fmt;

/// A wrapper on top [`diagnostic_msgs::msg::DiagnosticStatus`](https://docs.ros.org/en/rolling/p/diagnostic_msgs/interfaces/msg/DiagnosticStatus.html)
/// to provide helper functions such as [`summary`](DiagnosticStatusWrapper::summary),
/// [`add`](DiagnosticStatusWrapper::add) and [`merge_summary`](DiagnosticStatusWrapper::merge_summary).
#[derive(Clone, Default)]
pub struct DiagnosticStatusWrapper {
    /// Internal DiagnosticStatus message
    pub status: DiagnosticStatus,
}

impl DiagnosticStatusWrapper {
    /// Fills out the level and message fields of the DiagnosticStatus.
    ///
    /// # Arguments
    ///
    /// * `level` - Numerical level to assign to this Status (OK, Warn, Err).
    /// * `message` - Descriptive status message.
    ///
    /// # Examples
    ///
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w = DiagnosticStatusWrapper::default();
    /// w.summary(0, "test");
    /// assert_eq!(w.status.level, 0);
    /// assert_eq!(w.status.message, "test");
    /// ```
    pub fn summary<S: Into<String>>(&mut self, level: u8, message: S) {
        self.status.level = level;
        self.status.message = message.into();
    }

    /// Alternative version of [`summary`](DiagnosticStatusWrapper::summary) function to use formated arguments
    /// to be used by the [`summary`](crate::summary!) macro.
    ///
    /// # Example
    ///
    /// ```
    /// # use diagnostic_updater_rs::{summary, DiagnosticStatusWrapper};
    /// let mut w = DiagnosticStatusWrapper::default();
    /// let a = 1;
    /// w.summary_from_args(0, format_args!("test {}", a));
    /// assert_eq!(w.status.message, "test 1");
    /// ``````
    pub fn summary_from_args(&mut self, level: u8, args: fmt::Arguments<'_>) {
        self.status.level = level;
        self.status.message = format!("{}", args);
    }

    /// Merges a level and message with the existing ones.
    ///
    /// It is sometimes useful to merge two DiagnosticStatus messages. In that
    /// case, the key value pairs can be unioned, but the level and summary message
    /// have to be merged more intelligently. This function does the merge in
    /// an intelligent manner, combining the summary in `self`, with the one
    /// that is passed in.
    ///
    /// The combined level is the greater of the two levels to be merged.
    /// If both levels are non-zero (not OK), the messages are combined with a
    /// semicolon separator. If only one level is zero, and the other is
    /// non-zero, the message for the zero level is discarded. If both are
    /// zero, the new message is ignored.
    ///
    /// # Arguments
    ///
    /// * `level` - Numerical level to of the merged-in summary.
    /// * `message` - Descriptive status message for the merged-in summary.
    pub fn merge_summary<S: Into<String>>(&mut self, level: u8, message: S) {
        if (level > 0) == (self.status.level > 0) {
            if !self.status.message.is_empty() {
                self.status.message.push_str("; ");
                self.status.message.push_str(&message.into());
            }
        } else if level > self.status.level {
            self.status.message = message.into();
        }
        if level > self.status.level {
            self.status.level = level
        }
    }

    /// Version of [`merge_summary`](DiagnosticStatusWrapper::merge_summary) that merges in the summary from
    /// another DiagnosticStatus.
    pub fn merge_summary_from_status(&mut self, src: &DiagnosticStatus) {
        self.merge_summary(src.level, &src.message);
    }

    /// Version of [`merge_summary`](DiagnosticStatusWrapper::merge_summary) that merges in the summary from
    /// another DiagnosticStatusWrapper.
    pub fn merge_summary_from_wrapper(&mut self, src: &Self) {
        self.merge_summary(src.status.level, &src.status.message);
    }

    /// Clears the summary, setting the level to zero and the message to `""``.
    pub fn clear_summary(&mut self) {
        self.summary(0, "");
    }

    /// Clears the key-value pairs.
    ///
    /// The values vector containing the key-value pairs is cleared.
    pub fn clear(&mut self) {
        self.status.values.clear();
    }

    /// Adds a key-value pair.
    ///
    /// # Example
    ///
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w = DiagnosticStatusWrapper::default();
    /// w.add("key1", "value1");
    /// assert_eq!(w.status.values.len(), 1);
    /// assert_eq!(w.status.values[0].key, "key1");
    /// assert_eq!(w.status.values[0].value, "value1");
    /// ```
    pub fn add<S: Into<String>, T: ToString>(&mut self, key: S, value: T) {
        self.status.values.push(KeyValue {
            key: key.into(),
            value: value.to_string(),
        })
    }
}

/// Macro to use formated arguments for the [`DiagnosticStatusWrapper::summary()`] method
///
/// # Example
///
/// ```
/// use diagnostic_updater_rs::{summary, DiagnosticStatusWrapper};
///
/// let mut w = DiagnosticStatusWrapper::default();
/// let a = 1;
/// summary!(w, 0, "test {}", a);
/// assert_eq!(w.status.message, "test 1");
/// ```
#[macro_export]
macro_rules! summary {
    ($wrapper:expr, $level:expr, $($arg:tt)*) => {{
        $wrapper.summary_from_args($level, format_args!($($arg)*))
    }}
}

/// Macro to use formated arguments for the [`DiagnosticStatusWrapper::add()`] method
///
/// # Example
///
/// ```
/// use diagnostic_updater_rs::{add, DiagnosticStatusWrapper};
///
/// let mut w = DiagnosticStatusWrapper::default();
/// let a = 1;
/// add!(w, "key1", "value{}", a);
/// assert_eq!(w.status.values.len(), 1);
/// assert_eq!(w.status.values[0].key, "key1");
/// assert_eq!(w.status.values[0].value, "value1");
/// ```
#[macro_export]
macro_rules! add {
    ($wrapper:expr, $key:expr, $($arg:tt)*) => {{
        $wrapper.add($key, format_args!($($arg)*))
    }}
}
