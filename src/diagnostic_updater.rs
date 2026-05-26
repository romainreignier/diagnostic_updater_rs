use builtin_interfaces::msg;
use diagnostic_msgs::msg::DiagnosticArray;
use rclrs::{IntoPrimitiveOptions, Node, Publisher};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std_msgs::msg::Header;

use crate::diagnostic_status_wrapper::DiagnosticStatusWrapper;

pub trait DiagnosticTask {
    fn get_name(&self) -> String;
    fn run(&mut self, stat: &mut DiagnosticStatusWrapper);
}

pub struct FunctionDiagnosticTask {
    name: String,
    cb: Box<dyn Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync>,
}

impl FunctionDiagnosticTask {
    pub fn new<S, F>(name: S, cb: F) -> Self
    where
        S: Into<String>,
        F: Fn(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        Self {
            name: name.into(),
            cb: Box::new(cb),
        }
    }
}

impl DiagnosticTask for FunctionDiagnosticTask {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&mut self, stat: &mut DiagnosticStatusWrapper) {
        (self.cb)(stat)
    }
}

pub struct CompositeDiagnosticTask {
    name: String,
    tasks: Vec<Arc<Mutex<dyn DiagnosticTask + Send + Sync>>>,
}

impl CompositeDiagnosticTask {
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
        }
    }

    pub fn add_task<T>(&mut self, task: Arc<Mutex<T>>)
    where
        T: DiagnosticTask + 'static + Send + Sync,
    {
        self.tasks.push(task);
    }
}

impl DiagnosticTask for CompositeDiagnosticTask {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&mut self, stat: &mut DiagnosticStatusWrapper) {
        let mut combined_summary = DiagnosticStatusWrapper::default();
        let mut original_summary = DiagnosticStatusWrapper::default();
        original_summary.summary_from_status(stat);

        for task in &mut self.tasks {
            // Put the summary that was passed in.
            stat.summary_from_status(&original_summary);
            // Let the next task add entries and put its summary.
            task.lock().unwrap().run(stat);
            // Merge the new summary into the combined summary.
            combined_summary.merge_summary_from_status(stat);
        }

        // Copy the combined summary into the output.
        stat.summary_from_status(&combined_summary);
    }
}

type TaskFunction = dyn FnMut(&mut DiagnosticStatusWrapper) + 'static + Send + Sync;

struct DiagnosticTaskInternal {
    name: String,
    func: Arc<Mutex<TaskFunction>>,
}

impl DiagnosticTaskInternal {
    fn new<S, F>(name: S, func: Arc<Mutex<F>>) -> Self
    where
        S: Into<String>,
        F: FnMut(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        Self {
            name: name.into(),
            func,
        }
    }
}

impl DiagnosticTask for DiagnosticTaskInternal {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&mut self, stat: &mut DiagnosticStatusWrapper) {
        stat.status.name = self.name.clone();
        (self.func.lock().unwrap())(stat)
    }
}

struct UpdaterPrivate {
    node: Node,
    node_name: String,
    tasks: Vec<DiagnosticTaskInternal>,
    publisher: Publisher<DiagnosticArray>,
    period: Duration,
    hardware_id: Option<String>,
    warn_nohwid_done: bool,
}

impl UpdaterPrivate {
    fn new(node: Node, period: Duration, node_name: String) -> Self {
        let publisher = node.create_publisher("/diagnostics".keep_last(1)).unwrap();
        Self {
            node,
            node_name,
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
        F: FnMut(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        let int_task = DiagnosticTaskInternal::new(name, Arc::new(Mutex::new(cb)));
        self.add_internal(int_task)
    }

    fn add_internal(&mut self, task: DiagnosticTaskInternal) {
        self.tasks.push(task);
        self.added_task_callback(&self.tasks.last().unwrap());
    }

    fn add_task<T>(&mut self, task: Arc<Mutex<T>>)
    where
        T: DiagnosticTask + 'static + Send + Sync,
    {
        let task_closure = task.clone();
        self.add(
            task.lock().unwrap().get_name(),
            move |stat: &mut DiagnosticStatusWrapper| {
                task_closure.lock().unwrap().run(stat);
            },
        );
    }

    fn remove_by_name(&mut self, name: &str) -> bool {
        if let Some(pos) = self.tasks.iter().position(|item| item.get_name() == name) {
            self.tasks.remove(pos);
            true
        } else {
            false
        }
    }

    fn added_task_callback(&self, task: &DiagnosticTaskInternal) {
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
            s.status.name = format!("{}: {}", self.node_name, s.status.name);
        }

        let header = Header {
            frame_id: String::new(),
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
        for task in self.tasks.iter_mut() {
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
    /// Owns the rclrs parameter handles for the auto-discovery
    /// parameters declared by [`Updater::declare_aggregator_params`].
    /// Kept inside the updater because in rclrs 0.7
    /// `MandatoryParameter` / `OptionalParameter` undeclare the
    /// parameter on `Drop`, so the handle must outlive every consumer
    /// of the parameter service.
    _aggregator_handles: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

impl Updater {
    /// Creates an [Updater] object
    /// The publication rate is determined by the "~/diagnostic_updater.period" ros2 parameter.
    ///
    /// # Arguments
    ///
    /// * `node` - A rclrs node used for the publisher and the clock.
    ///
    /// # Errors
    ///
    /// * Returns an error if the internal parameter declaration fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use diagnostic_msgs::msg::DiagnosticStatus;
    /// use diagnostic_updater_rs::Updater;
    /// use rclrs::*;
    /// use std::time::Duration;
    ///
    /// let executor = Context::default().create_basic_executor();
    /// let node = executor.create_node("my_node").unwrap();
    /// let mut updater = Updater::new(node.clone()).unwrap();
    /// updater.set_hardware_id("none");
    /// updater.add("connection", |stat: &mut diagnostic_updater_rs::DiagnosticStatusWrapper| {
    ///     stat.summary(DiagnosticStatus::OK, "");
    /// });
    /// ```
    pub fn new(node: Node) -> Result<Self, rclrs::DeclarationError> {
        let period = node
            .declare_parameter("diagnostic_updater.period")
            .default(1.0)
            .mandatory()?;
        let period = Duration::from_secs_f64(period.get());
        let use_fqn = node
            .declare_parameter("diagnostic_updater.use_fqn")
            .default(false)
            .mandatory()?;
        let node_name = if use_fqn.get() {
            node.fully_qualified_name()
        } else {
            node.name()
        };
        Ok(Self::with_period_internal(node, period, node_name))
    }

    /// Creates an [Updater] object with a specified period.
    /// This version does not use a ROS parameter to set the period.
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
    /// use rclrs::*;
    /// use std::time::Duration;
    ///
    /// let executor = Context::default().create_basic_executor();
    /// let node = executor.create_node("my_node").unwrap();
    /// let mut updater = Updater::with_period(node.clone(), Duration::from_secs(1));
    /// updater.set_hardware_id("none");
    /// updater.add("connection", |stat: &mut diagnostic_updater_rs::DiagnosticStatusWrapper| {
    ///     stat.summary(DiagnosticStatus::OK, "");
    /// });
    /// ```
    pub fn with_period(node: Node, period: Duration) -> Self {
        let node_name = node.name();
        Self::with_period_internal(node, period, node_name)
    }

    fn with_period_internal(node: Node, period: Duration, node_name: String) -> Self {
        let mut s = Self {
            private: Arc::new(Mutex::new(UpdaterPrivate::new(node, period, node_name))),
            timer_thread: None,
            timer_running: Arc::new(AtomicBool::new(false)),
            _aggregator_handles: Vec::new(),
        };
        s.reset_timer();
        s
    }

    /// Declares the parameters used by `p_diagnostics_aggregator` to
    /// auto-discover this node via its parameter service:
    ///
    /// - `add_auto_diagnostics_path` (`string`, default `""`): the
    ///   path at which the node should be inserted in the aggregator
    ///   tree. Typically set via a launch file `<param>` override.
    ///   An empty value means the node is not registered.
    /// - `add_auto_diagnostics_stale_timeout` (`f64`, default `5.0`):
    ///   number of seconds without a `/diagnostics` message before
    ///   the aggregator marks this node stale.
    ///
    /// The parameter handles are retained by the updater so the
    /// declarations stay visible on the parameter service (rclrs 0.7
    /// undeclares parameters when their handle is dropped).
    ///
    /// # Examples
    /// ```
    /// use diagnostic_updater_rs::Updater;
    /// use rclrs::*;
    /// let executor = Context::default().create_basic_executor();
    /// let node = executor.create_node("my_node").unwrap();
    /// let mut updater = Updater::new(node.clone()).unwrap();
    /// updater.declare_aggregator_params().unwrap();
    /// ```
    pub fn declare_aggregator_params(&mut self) -> Result<(), rclrs::DeclarationError> {
        let node = self.private.lock().unwrap().node.clone();
        let path = node
            .declare_parameter::<Arc<str>>("add_auto_diagnostics_path")
            .default(Arc::from(""))
            .mandatory()?;
        let stale_timeout = node
            .declare_parameter::<f64>("add_auto_diagnostics_stale_timeout")
            .default(5.0)
            .mandatory()?;
        self._aggregator_handles.push(Box::new(path));
        self._aggregator_handles.push(Box::new(stale_timeout));
        Ok(())
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
    ///
    /// # Arguments
    /// * `hwid` - The hardware id string to set.
    ///
    /// # Examples
    /// ```
    /// use diagnostic_updater_rs::Updater;
    /// use rclrs::*;
    /// let executor = Context::default().create_basic_executor();
    /// let node = executor.create_node("my_node").unwrap();
    /// let mut updater = Updater::new(node.clone()).unwrap();
    /// updater.set_hardware_id("none");
    /// ```
    pub fn set_hardware_id<S: Into<String>>(&mut self, hwid: S) {
        self.private.lock().unwrap().hardware_id = Some(hwid.into())
    }

    /// Sets the hardware id string from a format-like list of arguments.
    /// See the [`set_hardware_id!`] macro for a more convenient way to call this function.
    ///
    /// # Arguments
    /// * `args` - A format-like list of arguments.
    ///
    /// # Examples
    /// ```
    /// use diagnostic_updater_rs::{set_hardware_id, Updater};
    /// use rclrs::*;
    /// let executor = Context::default().create_basic_executor();
    /// let node = executor.create_node("my_node").unwrap();
    /// let mut updater = Updater::new(node.clone()).unwrap();
    /// let device_id = 42;
    /// updater.set_hardware_id_from_args(format_args!("device_{}", device_id));
    /// // Alternatively, use the macro:
    /// set_hardware_id!(updater, "device_{}", device_id);
    /// ```
    pub fn set_hardware_id_from_args(&mut self, args: fmt::Arguments<'_>) {
        self.set_hardware_id(format!("{}", args));
    }

    /// Adds a closure embodied by a name that will be called to fill a [`DiagnosticStatusWrapper`].
    pub fn add<S, F>(&mut self, name: S, cb: F)
    where
        S: Into<String>,
        F: FnMut(&mut DiagnosticStatusWrapper) + 'static + Send + Sync,
    {
        self.private.lock().unwrap().add(name, cb);
    }

    pub fn add_task<T>(&mut self, task: Arc<Mutex<T>>)
    where
        T: DiagnosticTask + 'static + Send + Sync,
    {
        self.private.lock().unwrap().add_task(task);
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

/// Macro to set the hardware id from a format-like list of arguments.
///
/// # Examples
/// ```
/// use diagnostic_updater_rs::{set_hardware_id, Updater};
/// use rclrs::*;
/// let executor = Context::default().create_basic_executor();
/// let node = executor.create_node("my_node").unwrap();
/// let mut updater = Updater::new(node.clone()).unwrap();
/// let device_id = 42;
/// set_hardware_id!(updater, "device_{}", device_id);
/// ```
#[macro_export]
macro_rules! set_hardware_id {
    ($updater:expr, $($arg:tt)*) => {{
        $updater.set_hardware_id_from_args(format_args!($($arg)*))
    }}
}

#[cfg(test)]
mod tests {
    use rclrs::*;
    use serial_test::serial;
    use std::time::Duration;

    use super::*;

    // Run the test in sequential order to avoid several publications at the same time
    // on the global /diagnostics topic.

    #[test]
    #[serial]
    fn can_add_a_task() {
        let executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();
        let mut updater = Updater::with_period(node, Duration::from_secs(1));
        updater.add("test", |_| {});
        assert_eq!(updater.private.lock().unwrap().tasks.len(), 1);
    }

    #[test]
    #[serial]
    fn can_create_with_default_period() {
        let executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();
        let updater = Updater::new(node).unwrap();
        assert_eq!(updater.get_period(), Duration::from_secs(1));
    }

    #[test]
    #[serial]
    fn can_create_with_period_argument() {
        let executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();
        let updater = Updater::with_period(node, Duration::from_secs(2));
        assert_eq!(updater.get_period(), Duration::from_secs(2));
    }

    #[test]
    #[serial]
    fn status_name_uses_short_node_name_by_default() {
        let mut executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription("/diagnostics", move |msg: DiagnosticArray| {
                msgs_cb.lock().unwrap().push(msg);
            })
            .unwrap();

        let mut updater = Updater::new(node.clone()).unwrap();
        updater.add("test", |stat: &mut DiagnosticStatusWrapper| {
            stat.summary(0, "ok");
        });
        updater.force_update();
        // Drain pending subscription callbacks.
        for _ in 0..10 {
            executor.spin(SpinOptions::spin_once());
            if msgs.lock().unwrap().iter().any(|m| {
                m.status
                    .iter()
                    .any(|s| s.name == "diagnosed_node: test" && s.message == "ok")
            }) {
                break;
            }
        }

        let msgs = msgs.lock().unwrap();
        assert!(
            msgs.iter().any(|m| {
                m.status
                    .iter()
                    .any(|s| s.name == "diagnosed_node: test" && s.message == "ok")
            }),
            "expected a status named 'diagnosed_node: test', got: {:?}",
            msgs.iter()
                .flat_map(|m| m.status.iter().map(|s| s.name.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    #[serial]
    fn status_name_uses_fqn_when_parameter_is_set() {
        let mut executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        // Enable the FQN prefix before constructing the Updater.
        node.use_undeclared_parameters()
            .set("diagnostic_updater.use_fqn", true)
            .unwrap();

        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription("/diagnostics", move |msg: DiagnosticArray| {
                msgs_cb.lock().unwrap().push(msg);
            })
            .unwrap();

        let mut updater = Updater::new(node.clone()).unwrap();
        updater.add("test", |stat: &mut DiagnosticStatusWrapper| {
            stat.summary(0, "ok");
        });
        updater.force_update();
        // Drain pending subscription callbacks.
        for _ in 0..10 {
            executor.spin(SpinOptions::spin_once());
            if msgs.lock().unwrap().iter().any(|m| {
                m.status
                    .iter()
                    .any(|s| s.name == "/diagnosed_node: test" && s.message == "ok")
            }) {
                break;
            }
        }

        let msgs = msgs.lock().unwrap();
        assert!(
            msgs.iter().any(|m| {
                m.status
                    .iter()
                    .any(|s| s.name == "/diagnosed_node: test" && s.message == "ok")
            }),
            "expected a status named '/diagnosed_node: test', got: {:?}",
            msgs.iter()
                .flat_map(|m| m.status.iter().map(|s| s.name.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    #[serial]
    fn can_create_with_period_set_by_ros_parameter() {
        let executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        // Set the parameter to change the period
        node.use_undeclared_parameters()
            .set("diagnostic_updater.period", 3.0)
            .unwrap();

        let updater = Updater::new(node).unwrap();
        assert_eq!(updater.get_period(), Duration::from_secs(3));
    }

    #[test]
    #[serial]
    fn can_remove_a_task_by_name() {
        let executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();
        let mut updater = Updater::with_period(node, Duration::from_secs(1));
        assert!(!updater.remove_by_name("test"));
        updater.add("test", |_| {});
        assert!(updater.remove_by_name("test"));
    }

    #[test]
    #[serial]
    fn can_publish_diag_msg_on_start() {
        let mut executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        let msg_counter = Arc::new(Mutex::new(0));
        let counter_cb = msg_counter.clone();
        let _subscriber = node
            .create_subscription("/diagnostics", move |_msg: DiagnosticArray| {
                let mut counter = counter_cb.lock().unwrap();
                *counter += 1;
            })
            .unwrap();
        let mut updater = Updater::with_period(node.clone(), Duration::from_secs(1));
        updater.add("test", |_| {});
        executor.spin(SpinOptions::spin_once());
        assert_eq!(*msg_counter.lock().unwrap(), 1);
    }

    #[test]
    #[serial]
    fn can_publish_diag_msgs() {
        let mut executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription("/diagnostics", move |msg: DiagnosticArray| {
                (*msgs_cb.lock().unwrap()).push(msg.clone());
            })
            .unwrap();
        let period = Duration::from_millis(10);
        let mut updater = Updater::with_period(node.clone(), period);
        updater.add("test", |_| {});
        executor.spin(SpinOptions::spin_once());
        assert_eq!((*msgs.lock().unwrap()).len(), 1);
        assert_eq!((*msgs.lock().unwrap())[0].status[0].level, 0);
        assert_eq!(
            (*msgs.lock().unwrap())[0].status[0].message,
            "Node starting up"
        );
        for i in 0..3 {
            executor.spin(SpinOptions::spin_once());
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
        let mut executor = Context::default().create_basic_executor();
        let node = executor.create_node("diagnosed_node").unwrap();

        // Create diag subscriber to check the messages
        let msgs = Arc::new(Mutex::new(Vec::new()));
        let msgs_cb = msgs.clone();
        let _subscriber = node
            .create_subscription("/diagnostics", move |msg: DiagnosticArray| {
                (*msgs_cb.lock().unwrap()).push(msg.clone());
            })
            .unwrap();

        // Create the diagnostic updater
        let period = Duration::from_millis(10);
        let mut updater = Updater::with_period(node.clone(), period);
        updater.set_period(Duration::from_millis(50));
        updater.add("test", |_| {});

        // Startup message received
        executor.spin(SpinOptions::spin_once());
        assert_eq!((*msgs.lock().unwrap()).len(), 1);
    }
}
