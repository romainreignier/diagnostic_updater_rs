use std::sync::{Arc, Mutex};

use crate::diagnostic_updater::{CompositeDiagnosticTask, DiagnosticTask, Updater};
use crate::update_functions::{
    FrequencyStatus, FrequencyStatusParam, TimeStampStatus, TimeStampStatusParam,
};

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

struct TopicDiagnostic {
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
