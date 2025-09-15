use std::sync::{Arc, Mutex};

use crate::diagnostic_updater::{CompositeDiagnosticTask, DiagnosticTask, Updater};
use crate::update_functions::{
    FrequencyStatus, FrequencyStatusParam, TimeStampStatus, TimeStampStatusParam,
};

/// Trait implemented by message types that carry a [`std_msgs::msg::Header`].
///
/// Implement it once for each message type that you wrap with a
/// [`DiagnosedPublisher`] so the publisher can extract the stamp for the
/// [`TimeStampStatus`] diagnostic.
///
/// # Example
/// ```ignore
/// use diagnostic_updater_rs::HasHeader;
///
/// impl HasHeader for sensor_msgs::msg::Image {
///     fn header(&self) -> &std_msgs::msg::Header { &self.header }
/// }
/// ```
pub trait HasHeader {
    fn header(&self) -> &std_msgs::msg::Header;
}

fn builtin_time_to_nanos(stamp: &builtin_interfaces::msg::Time) -> i64 {
    (stamp.sec as i64) * 1_000_000_000 + (stamp.nanosec as i64)
}

// Built-in impl for the one header-bearing message type this crate already
// depends on. Downstream users add their own `impl HasHeader for ...` for
// other message types (sensor_msgs::msg::Image, etc.).
impl HasHeader for diagnostic_msgs::msg::DiagnosticArray {
    fn header(&self) -> &std_msgs::msg::Header {
        &self.header
    }
}

pub struct HeaderlessTopicDiagnostic {
    task: Arc<Mutex<CompositeDiagnosticTask>>,
    frequency_status: Arc<Mutex<FrequencyStatus>>,
}

impl<'a> HeaderlessTopicDiagnostic {
    pub fn new<S>(
        name: S,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
    ) -> Result<Self, rclrs::RclrsError>
    where
        S: Into<String>,
    {
        let frequency_status = Arc::new(Mutex::new(FrequencyStatus::new(freq_param)?));
        HeaderlessTopicDiagnostic::new_internal(name, diag, frequency_status)
    }

    pub fn with_clock<S>(
        name: S,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
        clock: rclrs::Clock,
    ) -> Result<Self, rclrs::RclrsError>
    where
        S: Into<String>,
    {
        let frequency_status = Arc::new(Mutex::new(
            FrequencyStatus::new(freq_param)?.with_clock(clock),
        ));
        HeaderlessTopicDiagnostic::new_internal(name, diag, frequency_status)
    }

    fn new_internal<S>(
        name: S,
        diag: &mut Updater,
        frequency_status: Arc<Mutex<FrequencyStatus>>,
    ) -> Result<Self, rclrs::RclrsError>
    where
        S: Into<String>,
    {
        let task = Arc::new(Mutex::new(CompositeDiagnosticTask::new(name)));
        let instance = Self {
            task: task.clone(),
            frequency_status: frequency_status.clone(),
        };
        task.lock().unwrap().add_task(frequency_status.clone());
        diag.add_task(instance.task.clone());
        Ok(instance)
    }

    pub fn tick(&mut self) {
        self.frequency_status.lock().unwrap().tick();
    }

    pub fn clear_window(&mut self) {
        self.frequency_status.lock().unwrap().clear();
    }

    pub fn add_task<T>(&mut self, task: Arc<Mutex<T>>)
    where
        T: DiagnosticTask + 'static + Send + Sync,
    {
        self.task.lock().unwrap().add_task(task);
    }
}

pub struct TopicDiagnostic {
    topic_diag: HeaderlessTopicDiagnostic,
    timestamp_status: Arc<Mutex<TimeStampStatus>>,
}

impl TopicDiagnostic {
    pub fn new<S>(
        name: S,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
        time_param: TimeStampStatusParam,
    ) -> Result<Self, rclrs::RclrsError>
    where
        S: Into<String>,
    {
        let timestamp_status = Arc::new(Mutex::new(TimeStampStatus::new(time_param)?));
        let topic_diag = HeaderlessTopicDiagnostic::new(name, diag, freq_param)?;
        topic_diag
            .task
            .lock()
            .unwrap()
            .add_task(timestamp_status.clone());
        Ok(Self {
            topic_diag,
            timestamp_status,
        })
    }

    pub fn with_clock<S>(
        name: S,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
        time_param: TimeStampStatusParam,
        clock: rclrs::Clock,
    ) -> Result<Self, rclrs::RclrsError>
    where
        S: Into<String>,
    {
        let timestamp_status = Arc::new(Mutex::new(
            TimeStampStatus::new(time_param)?.with_clock(clock.clone()),
        ));
        let topic_diag = HeaderlessTopicDiagnostic::with_clock(name, diag, freq_param, clock)?;
        topic_diag
            .task
            .lock()
            .unwrap()
            .add_task(timestamp_status.clone());
        Ok(Self {
            topic_diag,
            timestamp_status,
        })
    }

    pub fn tick(&mut self, stamp: &rclrs::Time) {
        self.timestamp_status.lock().unwrap().tick_with_time(stamp);
        self.topic_diag.tick();
    }

    /// Records a tick using a [`builtin_interfaces::msg::Time`] timestamp, the
    /// shape ROS message headers carry.
    pub fn tick_from_builtin(&mut self, stamp: &builtin_interfaces::msg::Time) {
        self.timestamp_status
            .lock()
            .unwrap()
            .tick(builtin_time_to_nanos(stamp));
        self.topic_diag.tick();
    }

    pub fn clear_window(&mut self) {
        self.topic_diag.clear_window();
    }

    pub fn add_task<T>(&mut self, task: Arc<Mutex<T>>)
    where
        T: DiagnosticTask + 'static + Send + Sync,
    {
        self.topic_diag.add_task(task);
    }
}

/// A [`TopicDiagnostic`] combined with a [`rclrs::Publisher`].
///
/// Wraps the publisher so each `publish(&msg)` call also records a
/// [`FrequencyStatus`] tick and a [`TimeStampStatus`] tick using the message's
/// header timestamp.
///
/// The message type `T` must implement [`HasHeader`].
pub struct DiagnosedPublisher<T>
where
    T: rosidl_runtime_rs::Message + HasHeader,
{
    publisher: rclrs::Publisher<T>,
    topic_diag: TopicDiagnostic,
}

impl<T> DiagnosedPublisher<T>
where
    T: rosidl_runtime_rs::Message + HasHeader,
{
    pub fn new(
        publisher: rclrs::Publisher<T>,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
        time_param: TimeStampStatusParam,
    ) -> Result<Self, rclrs::RclrsError> {
        let topic_diag =
            TopicDiagnostic::new(publisher.topic_name(), diag, freq_param, time_param)?;
        Ok(Self {
            publisher,
            topic_diag,
        })
    }

    pub fn with_clock(
        publisher: rclrs::Publisher<T>,
        diag: &mut Updater,
        freq_param: FrequencyStatusParam,
        time_param: TimeStampStatusParam,
        clock: rclrs::Clock,
    ) -> Result<Self, rclrs::RclrsError> {
        let topic_diag = TopicDiagnostic::with_clock(
            publisher.topic_name(),
            diag,
            freq_param,
            time_param,
            clock,
        )?;
        Ok(Self {
            publisher,
            topic_diag,
        })
    }

    /// Publishes the message and records frequency + timestamp diagnostics.
    ///
    /// The timestamp used by the [`TimeStampStatus`] is `msg.header().stamp`.
    pub fn publish(&mut self, msg: &T) -> Result<(), rclrs::RclrsError> {
        self.topic_diag.tick_from_builtin(&msg.header().stamp);
        self.publisher.publish(msg)
    }

    pub fn get_publisher(&self) -> rclrs::Publisher<T> {
        self.publisher.clone()
    }

    pub fn set_publisher(&mut self, publisher: rclrs::Publisher<T>) {
        self.publisher = publisher;
    }

    pub fn clear_window(&mut self) {
        self.topic_diag.clear_window();
    }

    pub fn add_task<U>(&mut self, task: Arc<Mutex<U>>)
    where
        U: DiagnosticTask + 'static + Send + Sync,
    {
        self.topic_diag.add_task(task);
    }
}
