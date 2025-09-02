use std::sync::{Arc, Mutex};

use crate::diagnostic_updater::{CompositeDiagnosticTask, DiagnosticTask, Updater};
use crate::update_functions::{FrequencyStatus, FrequencyStatusParam};

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
