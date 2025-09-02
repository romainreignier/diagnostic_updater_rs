use crate::diagnostic_status_wrapper::DiagnosticStatusWrapper;
use crate::diagnostic_updater::DiagnosticTask;
use rclrs::{RclrsError, Time};

pub struct FrequencyStatusParam<'a> {
    min_freq: &'a f64,
    max_freq: &'a f64,
    tolerance: f64,
    window_size: usize,
}

impl FrequencyStatusParam<'_> {
    pub fn new<'a>(min_freq: &'a f64, max_freq: &'a f64) -> FrequencyStatusParam<'a> {
        FrequencyStatusParam {
            min_freq,
            max_freq,
            tolerance: 0.1,
            window_size: 5,
        }
    }

    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    pub fn with_window_size(mut self, window_size: usize) -> Self {
        self.window_size = window_size;
        self
    }
}

fn nanos_to_secs(nanos: i64) -> f64 {
    (nanos as f64) * 1e-9
}

fn secs_to_nanos(secs: f64) -> i64 {
    (secs * 1e9) as i64
}

// TODO implement thread safety
// Maybe with internal struct protected by Mutex
pub struct FrequencyStatus<'a> {
    name: String,
    params: FrequencyStatusParam<'a>,
    count: usize,
    times: Vec<rclrs::Time>,
    seq_nums: Vec<usize>,
    hist_index: usize,
    debug_logger: rclrs::Logger,
    clock: rclrs::Clock,
}

impl<'a> FrequencyStatus<'a> {
    pub fn new(params: FrequencyStatusParam<'a>) -> Result<Self, RclrsError> {
        Self::with_name(params, "Frequency Status")
    }

    pub fn with_name<S>(params: FrequencyStatusParam<'a>, name: S) -> Result<Self, RclrsError>
    where
        S: Into<String>,
    {
        let logger = rclrs::Logger::new("FrequencyStatus_debug_logger")?;
        let clock = rclrs::Clock::system();
        let current_time = clock.now();
        let window_size = params.window_size;
        Ok(FrequencyStatus {
            name: name.into(),
            params,
            count: 0,
            times: vec![current_time; window_size],
            seq_nums: vec![0; window_size],
            hist_index: 0,
            debug_logger: logger,
            clock,
        })
    }

    pub fn with_clock(mut self, clock: rclrs::Clock) -> Self {
        self.clock = clock;
        self.clear();
        self
    }

    pub fn clear(&mut self) {
        self.count = 0;
        let current_time = self.clock.now();
        for t in &mut self.times {
            *t = current_time.clone();
        }
        for seq in &mut self.seq_nums {
            *seq = 0;
        }
        self.hist_index = 0;
    }

    pub fn tick(&mut self) {
        self.count += 1;
    }
}

impl DiagnosticTask for FrequencyStatus<'_> {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&mut self, stat: &mut DiagnosticStatusWrapper) {
        let curtime = self.clock.now();

        let curseq = self.count;
        let events = curseq - self.seq_nums[self.hist_index];
        let window = curtime.nsec - self.times[self.hist_index].nsec;
        let freq = events as f64 / nanos_to_secs(window);
        self.seq_nums[self.hist_index] = curseq;
        self.times[self.hist_index] = curtime;
        self.hist_index = (self.hist_index + 1) % self.params.window_size;

        if events == 0 {
            stat.summary(2, "No events recorded.");
        } else if freq < *self.params.min_freq * (1.0 - self.params.tolerance) {
            stat.summary(1, "Frequency too low.");
        } else if freq > *self.params.max_freq * (1.0 + self.params.tolerance) {
            stat.summary(1, "Frequency too high.");
        } else {
            stat.summary(0, "Desired frequency met");
        }

        stat.add("Events in window", events);
        stat.add("Events since startup", self.count);
        stat.add("Duration of window (s)", window);
        stat.add("Actual frequency (Hz)", freq);
        if *self.params.min_freq == *self.params.max_freq {
            stat.add("Target frequency (Hz)", *self.params.min_freq);
        }
        if *self.params.min_freq > 0.0 {
            stat.add(
                "Minimum acceptable frequency (Hz)",
                *self.params.min_freq * (1.0 - self.params.tolerance),
            );
        }
        if self.params.max_freq.is_finite() {
            stat.add(
                "Maximum acceptable frequency (Hz)",
                *self.params.max_freq * (1.0 + self.params.tolerance),
            );
        }
    }
}

pub struct TimeStampStatusParam {
    max_acceptable: f64,
    min_acceptable: f64,
}

impl TimeStampStatusParam {
    pub fn new() -> TimeStampStatusParam {
        TimeStampStatusParam {
            max_acceptable: 5.0,
            min_acceptable: -1.0,
        }
    }

    pub fn with_max_acceptable(mut self, max_acceptable: f64) -> Self {
        self.max_acceptable = max_acceptable;
        self
    }

    pub fn with_min_acceptable(mut self, min_acceptable: f64) -> Self {
        self.min_acceptable = min_acceptable;
        self
    }
}

pub struct TimeStampStatus {
    name: String,
    params: TimeStampStatusParam,
    early_count: usize,
    late_count: usize,
    zero_count: usize,
    zero_seen: bool,
    max_delta_ns: i64,
    min_delta_ns: i64,
    deltas_valid: bool,
    clock: rclrs::Clock,
}

impl TimeStampStatus {
    pub fn new(params: TimeStampStatusParam) -> Result<Self, RclrsError> {
        Self::with_name(params, "Timestamp Status")
    }

    pub fn with_name<S>(params: TimeStampStatusParam, name: S) -> Result<Self, RclrsError>
    where
        S: Into<String>,
    {
        let clock = rclrs::Clock::system();
        Ok(TimeStampStatus {
            name: name.into(),
            params,
            early_count: 0,
            late_count: 0,
            zero_count: 0,
            zero_seen: false,
            max_delta_ns: 0,
            min_delta_ns: 0,
            deltas_valid: false,
            clock,
        })
    }

    pub fn with_clock(mut self, clock: rclrs::Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn tick(&mut self, stamp_ns: i64) {
        if stamp_ns == 0 {
            self.zero_seen = true;
        } else {
            let delta = self.clock.now().nsec - stamp_ns;

            if !self.deltas_valid || delta > self.max_delta_ns {
                self.max_delta_ns = delta;
            }

            if !self.deltas_valid || delta < self.min_delta_ns {
                self.min_delta_ns = delta;
            }

            self.deltas_valid = true;
        }
    }

    pub fn tick_with_time(&mut self, time: &Time) {
        self.tick(time.nsec);
    }
}

impl DiagnosticTask for TimeStampStatus {
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn run(&mut self, stat: &mut DiagnosticStatusWrapper) {
        stat.summary(0, "Timestamps are reasonable.");
        if !self.deltas_valid {
            stat.summary(1, "No data since last update.");
        } else {
            if self.min_delta_ns < secs_to_nanos(self.params.min_acceptable) {
                stat.summary(2, "Timestamps too far in future seen.");
                self.early_count += 1;
            }

            if self.max_delta_ns > secs_to_nanos(self.params.max_acceptable) {
                stat.summary(2, "Timestamps too far in past seen.");
                self.late_count += 1;
            }

            if self.zero_seen {
                stat.summary(2, "Zero timestamp seen.");
                self.zero_count += 1;
            }
        }

        stat.add(
            "Earliest timestamp delay:",
            nanos_to_secs(self.min_delta_ns),
        );
        stat.add("Latest timestamp delay:", nanos_to_secs(self.max_delta_ns));
        stat.add(
            "Earliest acceptable timestamp delay:",
            self.params.min_acceptable,
        );
        stat.add(
            "Latest acceptable timestamp delay:",
            self.params.max_acceptable,
        );
        stat.add("Late diagnostic update count:", self.late_count);
        stat.add("Early diagnostic update count:", self.early_count);
        stat.add("Zero seen diagnostic update count:", self.zero_count);

        self.deltas_valid = false;
        self.min_delta_ns = 0;
        self.max_delta_ns = 0;
        self.zero_seen = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_frequency_status() {
        // From C++ test case
        let min_freq = 10.0;
        let max_freq = 20.0;

        let params = FrequencyStatusParam::new(&min_freq, &max_freq)
            .with_tolerance(0.5)
            .with_window_size(2);
        let mut freq_status = FrequencyStatus::new(params).unwrap();
        let mut stats: Vec<DiagnosticStatusWrapper> = vec![DiagnosticStatusWrapper::default(); 5];
        freq_status.tick();
        thread::sleep(Duration::from_millis(20));
        freq_status.run(&mut stats[0]); // Should be too fast, 20 ms for 1 tick, lower limit should be 33ms.
        thread::sleep(Duration::from_millis(50));
        freq_status.tick();
        freq_status.run(&mut stats[1]); // Should be good, 70 ms for 2 ticks, lower limit should be 66 ms.
        thread::sleep(Duration::from_millis(300));
        freq_status.tick();
        freq_status.run(&mut stats[2]); // Should be good, 350 ms for 2 ticks, upper limit should be 400 ms.
        thread::sleep(Duration::from_millis(150));
        freq_status.tick();
        freq_status.run(&mut stats[3]); // Should be too slow, 450 ms for 2 ticks, upper limit should be 400 ms.
        freq_status.clear();
        freq_status.run(&mut stats[4]); // Should be good, just cleared it.

        assert_eq!(
            1, stats[0].status.level,
            "max frequency exceeded but not reported"
        );
        assert_eq!(
            0, stats[1].status.level,
            "within max frequency but reported error"
        );
        assert_eq!(
            0, stats[2].status.level,
            "within min frequency but reported error"
        );
        assert_eq!(
            1, stats[3].status.level,
            "min frequency exceeded but not reported"
        );
        assert_eq!(2, stats[4].status.level, "freshly cleared should fail");
        assert_eq!(
            "", stats[0].status.name,
            "Name should not be set by FrequencyStatus"
        );
        assert_eq!(
            "Frequency Status",
            freq_status.get_name(),
            "Name should be \"Frequency Status\""
        );
    }

    #[test]
    fn test_timestamp_status() {
        let params = TimeStampStatusParam::new();
        let mut ts_status = TimeStampStatus::new(params).unwrap();

        let mut stats: Vec<DiagnosticStatusWrapper> = vec![DiagnosticStatusWrapper::default(); 5];
        ts_status.run(&mut stats[0]); // No data
        ts_status.tick(ts_status.clock.now().nsec + 2_000_000_000);
        ts_status.run(&mut stats[1]); // Too far in future
        ts_status.tick(ts_status.clock.now().nsec);
        ts_status.run(&mut stats[2]); // Now
        ts_status.tick(ts_status.clock.now().nsec - 4_000_000_000);
        ts_status.run(&mut stats[3]); // 4 seconds ago
        ts_status.tick(ts_status.clock.now().nsec - 6_000_000_000);
        ts_status.run(&mut stats[4]); // Too far in past

        assert_eq!(1, stats[0].status.level, "no data should return a warning");
        assert_eq!(2, stats[1].status.level, "too far future not reported");
        assert_eq!(0, stats[2].status.level, "now not accepted");
        assert_eq!(0, stats[3].status.level, "4 seconds ago not accepted");
        assert_eq!(2, stats[4].status.level, "too far past not reported");
        assert_eq!(
            "", stats[0].status.name,
            "Name should not be set by TimeStampStatus"
        );
        assert_eq!(
            "Timestamp Status",
            ts_status.get_name(),
            "Name should be \"Timestamp Status\""
        );
    }
}
