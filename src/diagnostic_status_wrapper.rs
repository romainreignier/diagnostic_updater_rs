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
    /// w.summary(diagnostic_msgs::msg::DiagnosticStatus::WARN, "test");
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::WARN);
    /// assert_eq!(w.status.message, "test");
    /// assert_eq!(w.status.values.len(), 0);
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

    /// Version of [`summary`](DiagnosticStatusWrapper::summary) that copies the level and message from
    /// another DiagnosticStatusWrapper.
    ///
    /// # Examples
    /////
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w1 = DiagnosticStatusWrapper::default();
    /// w1.summary(diagnostic_msgs::msg::DiagnosticStatus::WARN, "test");
    ///
    /// let mut w2 = DiagnosticStatusWrapper::default();
    /// w2.summary_from_status(&w1);
    /// assert_eq!(w2.status.level, diagnostic_msgs::msg::DiagnosticStatus::WARN);
    /// assert_eq!(w2.status.message, "test");
    /// ```
    pub fn summary_from_status(&mut self, src: &DiagnosticStatusWrapper) {
        self.summary(src.status.level, &src.status.message);
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w = DiagnosticStatusWrapper::default();
    /// w.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Was ok");
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::OK);
    /// assert_eq!(w.status.message, "Was ok");
    ///
    /// w.merge_summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Still ok");
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::OK);
    /// assert_eq!(w.status.message, "Was ok; Still ok");
    ///
    /// w.merge_summary(diagnostic_msgs::msg::DiagnosticStatus::WARN, "Warning");
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::WARN);
    /// assert_eq!(w.status.message, "Warning");
    ///
    /// w.merge_summary(diagnostic_msgs::msg::DiagnosticStatus::ERROR, "Error");
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::ERROR);
    /// assert_eq!(w.status.message, "Warning; Error");
    /// ```
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
    ///
    /// # Examples
    ////
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w1 = DiagnosticStatusWrapper::default();
    /// w1.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Was ok");
    ///
    /// let mut w2 = DiagnosticStatusWrapper::default();
    /// w2.summary(diagnostic_msgs::msg::DiagnosticStatus::ERROR, "Error");
    /// w1.merge_summary_from_status(&w2);
    /// assert_eq!(w2.status.level, diagnostic_msgs::msg::DiagnosticStatus::ERROR);
    /// assert_eq!(w2.status.message, "Error");
    /// ```
    pub fn merge_summary_from_status(&mut self, src: &DiagnosticStatusWrapper) {
        self.merge_summary(src.status.level, &src.status.message);
    }

    /// Version of [`merge_summary`](DiagnosticStatusWrapper::merge_summary) that merges in the summary from
    /// another DiagnosticStatusWrapper.
    ///
    /// # Examples
    ///
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w1 = DiagnosticStatusWrapper::default();
    /// w1.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Was ok");
    ///
    /// let mut w2 = DiagnosticStatusWrapper::default();
    /// w2.summary(diagnostic_msgs::msg::DiagnosticStatus::ERROR, "Error");
    ///
    /// w1.merge_summary_from_wrapper(&w2);
    /// assert_eq!(w1.status.level, diagnostic_msgs::msg::DiagnosticStatus::ERROR);
    /// assert_eq!(w1.status.message, "Error");
    /// ```
    pub fn merge_summary_from_wrapper(&mut self, src: &Self) {
        self.merge_summary(src.status.level, &src.status.message);
    }

    /// Alternative version of [`merge_summary`](DiagnosticStatusWrapper::merge_summary) function to use formated arguments
    /// to be used by the [`merge_summary`](crate::summary!) macro.
    pub fn merge_summary_from_args(&mut self, level: u8, args: fmt::Arguments<'_>) {
        self.merge_summary(level, format!("{}", args))
    }

    /// Clears the summary, setting the level to zero and the message to `""`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w = DiagnosticStatusWrapper::default();
    /// w.summary(diagnostic_msgs::msg::DiagnosticStatus::WARN, "test");
    /// w.clear_summary();
    /// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::OK);
    /// assert_eq!(w.status.message, "");
    /// assert_eq!(w.status.values.len(), 0);
    /// ```
    pub fn clear_summary(&mut self) {
        self.summary(0, "");
    }

    /// Clears the key-value pairs.
    ///
    /// The values vector containing the key-value pairs is cleared.
    ///
    /// # Example
    /// ```
    /// # use diagnostic_updater_rs::DiagnosticStatusWrapper;
    /// let mut w = DiagnosticStatusWrapper::default();
    /// w.add("key1", "value1");
    /// assert_eq!(w.status.values.len(), 1);
    /// w.clear();
    /// assert_eq!(w.status.values.len(), 0);
    /// ```
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
    ///
    /// w.add("key2", true);
    /// assert_eq!(w.status.values.len(), 2);
    /// assert_eq!(w.status.values[1].key, "key2");
    /// assert_eq!(w.status.values[1].value, "true");
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

/// Macro to use formated arguments for the [`DiagnosticStatusWrapper::merge_summary()`] method
///
/// # Example
/// ```
/// use diagnostic_updater_rs::{merge_summary, DiagnosticStatusWrapper};
/// let mut w = DiagnosticStatusWrapper::default();
/// w.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Was ok");
/// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::OK);
/// assert_eq!(w.status.message, "Was ok");
///
/// let a = 42;
/// merge_summary!(w, diagnostic_msgs::msg::DiagnosticStatus::OK, "Still ok {}", a);
/// assert_eq!(w.status.level, diagnostic_msgs::msg::DiagnosticStatus::OK);
/// assert_eq!(w.status.message, "Was ok; Still ok 42");
/// ``````
#[macro_export]
macro_rules! merge_summary {
    ($wrapper:expr, $level:expr, $($arg:tt)*) => {{
        $wrapper.merge_summary_from_args($level, format_args!($($arg)*))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_status_wrapper_is_empty() {
        let w = DiagnosticStatusWrapper::default();
        assert_eq!(w.status.level, 0);
        assert_eq!(w.status.message, "");
        assert_eq!(w.status.values.len(), 0);
    }
}
