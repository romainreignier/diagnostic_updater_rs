//! ROS 2 [`diagnostic_updater`](https://github.com/ros/diagnostics/tree/ros2/diagnostic_updater) in Rust to be used with [rclrs](https://crates.io/crates/rclrs)
//!
//! <div class="warning">
//!
//! While Timers are not implemented in rclrs, see [this PR]()https://github.com/ros2-rust/ros2_rust/pull/480,
//! a dedicated thread is used to update the diagnostics and publish them.
//!
//! </div>

mod diagnostic_status_wrapper;
mod diagnostic_updater;
mod publisher;
mod update_functions;

pub use crate::diagnostic_status_wrapper::DiagnosticStatusWrapper;
pub use crate::diagnostic_updater::{CompositeDiagnosticTask, FunctionDiagnosticTask, Updater};
pub use crate::publisher::{
    DiagnosedPublisher, HasHeader, HeaderlessTopicDiagnostic, TopicDiagnostic,
};
pub use crate::update_functions::{
    FrequencyStatus, FrequencyStatusParam, TimeStampStatus, TimeStampStatusParam,
};
