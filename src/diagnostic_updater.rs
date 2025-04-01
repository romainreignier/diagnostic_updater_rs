use builtin_interfaces::msg;
use diagnostic_msgs::msg::DiagnosticArray;
use rclrs::{Node, Publisher, ToLogParams, QOS_PROFILE_DEFAULT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std_msgs::msg::Header;

use crate::diagnostic_status_wrapper::DiagnosticStatusWrapper;

struct DiagnosticTask {
    name: String,
    cb: Box<dyn Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync>,
}

impl DiagnosticTask {
    fn new<S, F>(name: S, cb: F) -> Self
    where
        S: Into<String>,
        F: Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        Self {
            name: name.into(),
            cb: Box::new(cb),
        }
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&self, stat: &mut DiagnosticStatusWrapper) {
        (self.cb)(stat)
    }
}

struct UpdaterPrivate {
    node: Arc<Node>,
    tasks: Vec<DiagnosticTask>,
    publisher: Arc<Publisher<DiagnosticArray>>,
    period: Duration,
    hardware_id: Option<String>,
    warn_nohwid_done: bool,
}

impl UpdaterPrivate {
    fn new(node: Arc<Node>, period: Duration) -> Self {
        let publisher = node
            .create_publisher("/diagnostics", QOS_PROFILE_DEFAULT)
            .unwrap();
        Self {
            node,
            period,
            hardware_id: None,
            publisher,
            tasks: Vec::new(),
            warn_nohwid_done: false,
        }
    }

    fn add<S, F>(&mut self, name: S, cb: F)
    where
        S: Into<String>,
        F: Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        let task = DiagnosticTask::new(name, cb);
        self.added_task_callback(&task);
        self.tasks.push(task);
    }

    fn remove_by_name(&mut self, name: &str) -> bool {
        if let Some(pos) = self.tasks.iter().position(|item| item.name == name) {
            self.tasks.remove(pos);
            true
        } else {
            false
        }
    }

    fn added_task_callback(&self, task: &DiagnosticTask) {
        let mut status = DiagnosticStatusWrapper::default();
        status.status.name = task.get_name();
        status.summary(0, "Node starting up");
        self.publish_status(&status)
    }

    fn publish_status(&self, status: &DiagnosticStatusWrapper) {
        let status_vec = vec![status.clone()];
        self.publish(status_vec)
    }

    fn publish(&self, mut status_vec: Vec<DiagnosticStatusWrapper>) {
        for s in &mut status_vec {
            s.status.name = format!("{}: {}", self.node.name(), s.status.name);
        }

        let header = Header {
            frame_id: "map".to_string(),
            stamp: msg::Time {
                sec: (self.node.get_clock().now().nsec / 10_i64.pow(9)) as i32,
                nanosec: (self.node.get_clock().now().nsec % 10_i64.pow(9)) as u32,
            },
        };
        let msg = DiagnosticArray {
            header,
            status: status_vec.iter().map(|item| item.status.clone()).collect(),
        };
        self.publisher.publish(msg).unwrap();
    }

    fn broadcast(&self, level: u8, message: &str) {
        let mut status_vec = Vec::new();
        for task in self.tasks.iter() {
            let mut w = DiagnosticStatusWrapper::default();
            w.status.name = task.get_name();
            w.summary(level, message);
            status_vec.push(w);
        }
        self.publish(status_vec);
    }

    /// Causes the diagnostics to update if the inter-update interval has been exceeded.
    fn update(&mut self) {
        let mut warn_nohwid = self.hardware_id.is_none();
        let mut status_vec = Vec::new();
        for task in self.tasks.iter() {
            let mut w = DiagnosticStatusWrapper::default();
            w.status.name = task.get_name();
            w.status.level = 2;
            w.status.message = "No message was set".to_string();
            if let Some(hwid) = &self.hardware_id {
                w.status.hardware_id = hwid.clone();
            }

            task.run(&mut w);

            if w.status.level != 0 {
                warn_nohwid = false;
            }

            status_vec.push(w);
        }

        if warn_nohwid && !self.warn_nohwid_done {
            self.warn_nohwid_done = true;
            rclrs::log_warn!(self.node.logger(), "diagnostic_updater: No HW_ID was set. This is probably a bug. Please report it. For devices that do not have a HW_ID, set this value to 'none'. This warning only occurs once all diagnostics are OK. It is okay to wait until the device is open before calling setHardwareID.");
        }
        self.publish(status_vec);
    }
}

/// Collects the diagnostic messages and to publishes them.
pub struct Updater {
    private: Arc<Mutex<UpdaterPrivate>>,
    timer_thread: Option<JoinHandle<()>>,
    timer_running: Arc<AtomicBool>,
}

impl Updater {
    /// Creates an [Updater] object
    ///
    /// # Arguments
    ///
    /// * `node` - A rclrs node used for the publisher and the clock.
    /// * `period` - Period of publication of the diagnostics.
    ///
    /// # Examples
    ///
    /// ```
    /// use diagnostic_msgs::msg::DiagnosticStatus;
    /// use diagnostic_updater_rs::Updater;
    /// use std::time::Duration;
    ///
    /// let context = rclrs::Context::new(std::env::args()).unwrap();
    /// let node = rclrs::Node::new(&context, "my_node").unwrap();
    /// let mut updater = Updater::new(node.clone(), Duration::from_secs(1));
    /// updater.set_hardware_id("none");
    /// updater.add("connection", |mut stat| {
    ///     stat.summary(DiagnosticStatus::OK, "");
    /// });
    /// ```
    pub fn new(node: Arc<Node>, period: Duration) -> Self {
        let mut s = Self {
            private: Arc::new(Mutex::new(UpdaterPrivate::new(node, period))),
            timer_thread: None,
            timer_running: Arc::new(AtomicBool::new(false)),
        };
        s.reset_timer();
        s
    }

    /// Returns the interval between updates.
    pub fn get_period(&self) -> Duration {
        self.private.lock().unwrap().period
    }

    /// Sets the period of publication and restart the internal periodic thread.
    pub fn set_period(&mut self, period: Duration) {
        self.private.lock().unwrap().period = period;
        self.reset_timer();
    }

    /// Sets the hardware id string
    pub fn set_hardware_id<S: Into<String>>(&mut self, hwid: S) {
        self.private.lock().unwrap().hardware_id = Some(hwid.into())
    }

    /// Adds a closure embodied by a name that will be called to fill a [`DiagnosticStatusWrapper`].
    pub fn add<S, F>(&mut self, name: S, cb: F)
    where
        S: Into<String>,
        F: Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        self.private.lock().unwrap().add(name, cb);
    }

    /// Outputs a message on all the known DiagnosticStatus.
    ///
    /// Useful if something drastic is happening such as shutdown or a self-test.
    pub fn broadcast(&self, level: u8, message: &str) {
        self.private.lock().unwrap().broadcast(level, message)
    }

    /// Remove a task based on its name.
    pub fn remove_by_name(&mut self, name: &str) -> bool {
        self.private.lock().unwrap().remove_by_name(name)
    }

    /// Forces to send out an update for all known DiagnosticStatus.
    pub fn force_update(&mut self) {
        self.private.lock().unwrap().update()
    }

    fn reset_timer(&mut self) {
        if let Some(thread) = self.timer_thread.take() {
            self.timer_running.store(false, Ordering::Relaxed);
            thread.join().unwrap();
        }
        let private = self.private.clone();

        self.timer_running.store(true, Ordering::Relaxed);
        let thread_timer_running = Arc::clone(&self.timer_running);
        self.timer_thread = Some(thread::spawn(move || {
            let mut next_update = Instant::now() + private.lock().unwrap().period;
            while thread_timer_running.load(Ordering::Relaxed) {
                if next_update > Instant::now() {
                    std::thread::sleep(next_update - Instant::now());
                }
                let mut private = private.lock().unwrap();
                next_update += private.period;
                private.update();
            }
            println!("End of timer thread");
        }));
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use std::time::Duration;

    use super::*;

    // Run the test in sequential order to avoid several publications at the same time
    // on the global /diagnostics topic.

    #[test]
    #[serial]
    fn can_add_a_task() {
        let context = rclrs::Context::new(std::env::args()).unwrap();
        let node = rclrs::Node::new(&context, "diagnosed_node").unwrap();
        let mut updater = Updater::new(node, Duration::from_secs(1));
        updater.add("test", |_| {});
        assert_eq!(updater.private.lock().unwrap().tasks.len(), 1);
    }

    #[test]
    #[serial]
    fn can_remove_a_task_by_name() {
        let context = rclrs::Context::new(std::env::args()).unwrap();
        let node = rclrs::Node::new(&context, "diagnosed_node").unwrap();
        let mut updater = Updater::new(node, Duration::from_secs(1));
        assert!(!updater.remove_by_name("test"));
        updater.add("test", |_| {});
        assert!(updater.remove_by_name("test"));
    }

    #[test]
    #[serial]
    fn can_publish_diag_msg_on_start() {
        let context = rclrs::Context::new(std::env::args()).unwrap();
        let node = rclrs::Node::new(&context, "diagnosed_node").unwrap();

        let msg_counter = Arc::new(Mutex::new(0));
        let counter_cb = msg_counter.clone();
        let _subscriber = node
            .create_subscription(
                "/diagnostics",
                QOS_PROFILE_DEFAULT,
                move |_msg: DiagnosticArray| {
                    let mut counter = counter_cb.lock().unwrap();
                    *counter += 1;
                },
            )
            .unwrap();
        let mut updater = Updater::new(node.clone(), Duration::from_secs(1));
        updater.add("test", |_| {});
        rclrs::spin_once(node, None).unwrap();
        assert_eq!(*msg_counter.lock().unwrap(), 1);
    }

    #[test]
    #[serial]
    fn can_publish_diag_msgs() {
        let context = rclrs::Context::new(std::env::args()).unwrap();
        let node = rclrs::Node::new(&context, "diagnosed_node").unwrap();

        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription(
                "/diagnostics",
                QOS_PROFILE_DEFAULT,
                move |msg: DiagnosticArray| {
                    (*msgs_cb.lock().unwrap()).push(msg.clone());
                },
            )
            .unwrap();
        let period = Duration::from_millis(10);
        let mut updater = Updater::new(node.clone(), period);
        updater.add("test", |_| {});
        rclrs::spin_once(node.clone(), None).unwrap();
        assert_eq!((*msgs.lock().unwrap()).len(), 1);
        assert_eq!((*msgs.lock().unwrap())[0].status[0].level, 0);
        assert_eq!(
            (*msgs.lock().unwrap())[0].status[0].message,
            "Node starting up"
        );
        for i in 0..3 {
            rclrs::spin_once(node.clone(), None).unwrap();
            assert!((*msgs.lock().unwrap()).len() >= i + 2);
            assert_eq!((*msgs.lock().unwrap())[i + 1].status[0].level, 2);
            assert_eq!(
                (*msgs.lock().unwrap())[i + 1].status[0].message,
                "No message was set"
            );
        }
    }

    #[test]
    #[serial]
    fn can_change_period() {
        // Create ROS Node
        let context = rclrs::Context::new(std::env::args()).unwrap();
        let node = rclrs::Node::new(&context, "diagnosed_node").unwrap();

        // Create diag subscriber to check the messages
        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription(
                "/diagnostics",
                QOS_PROFILE_DEFAULT,
                move |msg: DiagnosticArray| {
                    (*msgs_cb.lock().unwrap()).push(msg.clone());
                },
            )
            .unwrap();

        // Create the diagnostic updater
        let period = Duration::from_millis(10);
        let mut updater = Updater::new(node.clone(), period);
        updater.set_period(Duration::from_millis(50));
        updater.add("test", |_| {});

        // Startup message received
        rclrs::spin_once(node.clone(), None).unwrap();
        assert_eq!((*msgs.lock().unwrap()).len(), 1);
    }
}
