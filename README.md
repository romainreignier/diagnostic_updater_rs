# ROS 2 diagnostic_updater for Rust

Rust implementation of the [C++ `diagnostic_updater``](https://github.com/ros/diagnostics/tree/ros2/diagnostic_updater).

## Example

```rust
use diagnostic_msgs::msg::DiagnosticStatus;
use diagnostic_updater_rs::Updater;
use std::time::Duration;

let context = rclrs::Context::new(std::env::args()).unwrap();
let node = rclrs::Node::new(&context, "my_node").unwrap();
let mut updater = Updater::new(node.clone(), Duration::from_secs(1));
updater.set_hardware_id("none");
updater.add("connection", |mut stat| {
    stat.summary(DiagnosticStatus::OK, "");
});
```