use std::{sync::atomic::Ordering, thread};

use flume::Sender;

use crate::{hello_world::JobsReply, ms, PROC_FLAG};

pub fn process_incoming_job(jobs_reply: JobsReply, complete_send: Sender<String>) {
    let proc_flag = PROC_FLAG.get().unwrap();
    proc_flag.store(true, Ordering::SeqCst);
    // simulate work
    for (idx, job) in jobs_reply.jobs.iter().enumerate() {
        tracing::info!(idx, "Processing job {}", job.id);
        // println!("{}", job.file);
        // let file = read_link
        thread::sleep(ms(1000));
        let _ = complete_send.send(job.id.clone());
    }
    proc_flag.store(false, Ordering::SeqCst);
}
