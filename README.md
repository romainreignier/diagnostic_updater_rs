# ROS 2 diagnostic_updater for Rust

Rust implementation of the [C++ `diagnostic_updater`](https://github.com/ros/diagnostics/tree/ros2/diagnostic_updater) using [rclrs](https://crates.io/crates/rclrs) ROS 2 Rust bindings.

## Example

```rust
use diagnostic_msgs::msg::DiagnosticStatus;
use diagnostic_updater_rs::Updater;
use rclrs::*;
use std::time::Duration;
let executor = Context::default().create_basic_executor();
let node = executor.create_node("my_node").unwrap();
let mut updater = Updater::new(node.clone()).unwrap();
updater.set_hardware_id("none");
updater.add("connection", |stat: &mut diagnostic_updater_rs::DiagnosticStatusWrapper| {
    stat.summary(DiagnosticStatus::OK, "");
});
```
