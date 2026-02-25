use std::{sync::atomic::Ordering, thread};

use flume::Sender;

use crate::{backtesting::JobsReply, ms, PROC_FLAG};

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
    proc_flag.store(false, Ordering::SeqCst);
}
