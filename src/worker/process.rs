use std::{sync::atomic::Ordering, thread};

use flume::Sender;

use crate::{backtesting::JobsReply, ms, PROC_FLAG};

/// # process_incoming_job
///
/// Handles incoming job and does work.
/// Note: currently only simulates doing heavy compute work.
/// * `jobs_reply` - the response we get from the server of jobs to work on.
/// * `complete_send` - channel to send completed work over.
pub fn process_incoming_job(jobs_reply: JobsReply, complete_send: Sender<String>) {
    let Some(proc_flag) = PROC_FLAG.get() else {
        tracing::error!("No PROC_FLAG value set");
        return;
    };
    proc_flag.store(true, Ordering::SeqCst);

    // simulate compute heavy work to do.
    for (idx, job) in jobs_reply.jobs.iter().enumerate() {
        tracing::info!(idx, "Processing job {}", job.id);
        thread::sleep(ms(1000));
        let _ = complete_send.send(job.id.clone());
    }

    // set our processing flag to false to signify we are available for work.
    proc_flag.store(false, Ordering::SeqCst);
}
