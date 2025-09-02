use diagnostic_msgs::msg::DiagnosticStatus;
use diagnostic_updater_rs::{
    add, summary, CompositeDiagnosticTask, DiagnosticStatusWrapper, FunctionDiagnosticTask,
    HeaderlessTopicDiagnostic, Updater,
};
use rclrs::{log_error, ToLogParams};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc, Mutex,
};

static TIME_TO_LAUNCH: AtomicI64 = AtomicI64::new(11);

fn main() {
    let context = rclrs::Context::new(std::env::args()).unwrap();
    let node = rclrs::Node::new(&context, "diagnostic_updater_example").unwrap();

    // The Updater class advertises to /diagnostics, and has a
    // ~diagnostic_period parameter that says how often the diagnostics
    // should be published.
    let mut updater = Updater::new(node.clone()).unwrap();

    // The diagnostic_updater::Updater class will fill out the hardware_id
    // field of the diagnostic_msgs::msg::DiagnosticStatus message. You need
    // to use the set_hardware_id() method to set the hardware ID.
    //
    // The hardware ID should be able to identify the specific device you are
    // working with. If it is not appropriate to fill out a hardware ID in
    // your case, you should call set_hardware_id("none") to avoid warnings.
    // (A warning will be generated as soon as your node updates with no
    // non-OK statuses.)
    updater.set_hardware_id("none");

    // Diagnostic tasks are added to the Updater. They will later be run when
    // the updater decides to update.
    // The add method takes a name and a function or closure.
    updater.add("Function updater", dummy_diagnostic);
    let ds = DummyStruct;
    updater.add("Method updater", move |stat| ds.produce_diagnostics(stat));

    // Internally, updater.add converts its arguments into a DiagnosticTask.
    // Sometimes it can be useful to work directly with DiagnosticTasks. Look
    // at FrequencyStatus and TimestampStatus in update_functions.rs for a
    // real-life example of how to use the DiagnosticTask trait.

    // Alternatively, a FunctionDiagnosticTask is a struct implementing the
    // DiagnosticTask trait that can be used to create a DiagnosticTask from
    // a function. This will be useful when combining multiple diagnostic
    // tasks using a CompositeDiagnosticTask.
    let lower = Arc::new(Mutex::new(FunctionDiagnosticTask::new(
        "Lower-bound check",
        check_lower_bound,
    )));
    let upper = Arc::new(Mutex::new(FunctionDiagnosticTask::new(
        "Upper-bound check",
        check_upper_bound,
    )));

    // If you want to merge the outputs of two diagnostic tasks together, you
    // can create a CompositeDiagnosticTask, also a derived class from
    // DiagnosticTask. For example, we could combine the upper and lower
    // bounds check into a single DiagnosticTask.
    let bounds = Arc::new(Mutex::new(CompositeDiagnosticTask::new("Bound check")));
    bounds.lock().unwrap().add_task(lower.clone());
    bounds.lock().unwrap().add_task(upper);

    // We can then add the CompositeDiagnosticTask to our Updater. When it is
    // run, the overall name will be the name of the composite task, i.e.,
    // "Bound check". The summary will be a combination of the summary of the
    // lower and upper tasks (see DiagnosticStatusWrapper::merge_summary for
    // details on how the merging is done).
    // The lists of key-value pairs will be concatenated.
    updater.add_task(bounds);

    // You can broadcast a message in all the DiagnosticStatus if your node
    // is in a special state.
    updater.broadcast(0, "Doing important initialization stuff.");

    let pub1 = node
        .create_publisher::<std_msgs::msg::Bool>(
            "topic1",
            rclrs::QoSProfile::default().keep_last(10),
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Some diagnostic tasks are very common, such as checking the rate
    // at which a topic is publishing, or checking that timestamps are
    // sufficiently recent. FrequencyStatus and TimestampStatus can do these
    // checks for you.
    //
    // Usually you would instantiate them via a HeaderlessTopicDiagnostic
    // (FrequencyStatus only, for topics that do not contain a header) or a
    // TopicDiagnostic (FrequencyStatus and TimestampStatus, for topics that
    // do contain a header).
    //
    // Some values are passed to the constructor as pointers. If these values
    // are changed, the FrequencyStatus/TimestampStatus will start operating
    // with the new values.
    //
    // Refer to FrequencyStatusParam and TimestampStatusParam documentation for
    // details on what the parameters mean:
    let min_freq = Arc::new(Mutex::new(0.5)); // If you update these values, the
    let max_freq = Arc::new(Mutex::new(2.0)); // HeaderlessTopicDiagnostic will use the new values.
    let mut pub1_freq = HeaderlessTopicDiagnostic::new(
        "topic1",
        &mut updater,
        diagnostic_updater_rs::FrequencyStatusParam::new(min_freq, max_freq)
            .with_tolerance(0.1)
            .with_window_size(10),
    )
    .unwrap();

    // Note that TopicDiagnostic, HeaderlessDiagnosedPublisher,
    // HeaderlessDiagnosedPublisher and DiagnosedPublisher all descend from
    // CompositeDiagnosticTask, so you can add your own fields to them using
    // the add_task method.
    //
    // Each time pub1_freq is updated, lower will also get updated and its
    // output will be merged with the output from pub1_freq.
    // pub1_freq.add_task(lower); // (This wouldn't work if lower was stateful).

    // If we know that the state of the node just changed, we can force an
    // immediate update.
    updater.force_update();

    // We can remove a task by refering to its name.
    if !updater.remove_by_name("Bound check") {
        log_error!(
            node.logger(),
            "The Bound check task was not found when trying to remove it."
        );
    }

    while context.ok() {
        let mut msg = std_msgs::msg::Bool::default();

        // Calls to pub1 have to be accompanied by calls to pub1_freq to keep
        // the statistics up to date.
        msg.data = false;
        pub1.publish(msg).unwrap();
        pub1_freq.tick();

        // Update TIME_TO_LAUNCH
        let time_to_launch = TIME_TO_LAUNCH.fetch_add(-1, Ordering::SeqCst);
        if time_to_launch < 0 {
            TIME_TO_LAUNCH.store(11, Ordering::SeqCst);
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn dummy_diagnostic(stat: &mut DiagnosticStatusWrapper) {
    // DiagnosticStatusWrapper is a wrapper above
    // diagnostic_msgs::msg::DiagnosticStatus to provide a set of convenience
    // methods.

    let time_to_launch = TIME_TO_LAUNCH.load(Ordering::SeqCst);

    // summary and summary! set the level and message.
    if time_to_launch < 10 {
        // summary! macro for formatted text.
        summary!(
            stat,
            DiagnosticStatus::ERROR,
            "Buckle your seat belt. Launch in {} seconds!",
            time_to_launch,
        );
    } else {
        // summary for unformatted text.
        stat.summary(
            DiagnosticStatus::OK,
            "Launch is in a long time. Have a soda.",
        );
    }

    // add and add! are used to append key-value pairs.
    stat.add("Diagnostic Name", "dummy");
    // add transparently handles conversion to string (using ToString trait).
    stat.add("Time to Launch", time_to_launch);
    // add! macro allows arbitrary print! style formatting.
    add!(
        stat,
        "Geeky thing to say",
        "The square of the time to launch {} is {}",
        time_to_launch,
        time_to_launch * time_to_launch,
    );
}

struct DummyStruct;

impl DummyStruct {
    fn produce_diagnostics(&self, stat: &mut DiagnosticStatusWrapper) {
        stat.summary(
            diagnostic_msgs::msg::DiagnosticStatus::WARN,
            "This is a silly updater.",
        );

        stat.add("Stupidicity of this updater", 1000.);
    }
}

fn check_lower_bound(stat: &mut DiagnosticStatusWrapper) {
    let time_to_launch = TIME_TO_LAUNCH.load(Ordering::SeqCst);

    if time_to_launch > 5 {
        stat.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Lower-bound OK");
    } else {
        stat.summary(diagnostic_msgs::msg::DiagnosticStatus::ERROR, "Too low");
    }

    stat.add("Low-Side Margin", time_to_launch - 5);
}

fn check_upper_bound(stat: &mut DiagnosticStatusWrapper) {
    let time_to_launch = TIME_TO_LAUNCH.load(Ordering::SeqCst);

    if time_to_launch < 10 {
        stat.summary(diagnostic_msgs::msg::DiagnosticStatus::OK, "Upper-bound OK");
    } else {
        stat.summary(diagnostic_msgs::msg::DiagnosticStatus::WARN, "Too high");
    }

    stat.add("Top-Side Margin", 10 - time_to_launch);
}
