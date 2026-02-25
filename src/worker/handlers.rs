use std::{error::Error, sync::atomic::Ordering};

use flume::Sender;
use tonic::transport::Channel;

use crate::{
    hello_world::{
        processor_client::ProcessorClient, JobsReply, JobsRequest, StatusRequest, WorkerStatus,
    },
    CONNECTED, PROC_FLAG,
};

/// send_status
/// Sends the status of the current processor.
pub async fn send_status(client: &mut ProcessorClient<Channel>) {
    let Some(processing_flag) = PROC_FLAG.get() else {
        tracing::error!("Unable to retrieve PROC_FLAG OnceLock");
        return;
    };

    let is_proc = processing_flag.load(Ordering::SeqCst);

    if is_proc {
        let status = StatusRequest {
            status: WorkerStatus::Running.into(),
        };

        let _ = client
            .send_status(status)
            .await
            .inspect_err(|why| tracing::error!("{why:?}"));
    }
}

pub async fn handle_job(client: &mut ProcessorClient<Channel>, reply_send: Sender<JobsReply>) {
    let cpu_count = num_cpus::get() / 2;

    let req = StatusRequest {
        status: WorkerStatus::Idle.into(),
    };

    if let Err(why) = client.send_status(req).await {
        let Some(conn_status) = CONNECTED.get() else {
            tracing::error!("Unable to retrive CONNECTED Atomic Bool");
            return;
        };

        let is_connected = conn_status.load(Ordering::SeqCst);
        if is_connected {
            tracing::error!("Unable to send status: {:?}", why.source());
        }
        conn_status.store(false, Ordering::SeqCst);
    }

    let request = tonic::Request::new(JobsRequest {
        cores: cpu_count as i32,
    });

    if let Ok(response) = client.request_jobs(request).await {
        let _ = reply_send
            .send_async(response.into_inner())
            .await
            .inspect_err(|why| tracing::error!("{why:?}"));
    };
}
