use std::sync::atomic::Ordering;

use flume::Sender;
use tonic::transport::Channel;

use crate::{
    hello_world::{
        processor_client::ProcessorClient, JobsReply, JobsRequest, StatusRequest, WorkerStatus,
    },
    CONNECTED, PROC_FLAG,
};

pub async fn send_status(client: &mut ProcessorClient<Channel>) {
    let processing_flag = PROC_FLAG.get().unwrap();
    let is_proc = processing_flag.load(Ordering::SeqCst);

    if is_proc {
        let status = StatusRequest {
            status: WorkerStatus::Running.into(),
        };

        client.send_status(status).await.unwrap();
    }
}

pub async fn handle_job(client: &mut ProcessorClient<Channel>, reply_send: Sender<JobsReply>) {
    let cpu_count = num_cpus::get() / 2;

    let req = StatusRequest {
        status: WorkerStatus::Idle.into(),
    };

    if let Err(why) = client.send_status(req).await {
        let conn_status = CONNECTED.get().unwrap();
        let is_conn = conn_status.load(Ordering::SeqCst);
        if is_conn {
            tracing::error!("{why:?}");
        }
        conn_status.store(false, Ordering::SeqCst);
    }

    let request = tonic::Request::new(JobsRequest {
        cores: cpu_count as i32,
    });

    let response = client.request_jobs(request).await;
    match response {
        Ok(res) => {
            reply_send
                .send_async(res.into_inner())
                .await
                .expect("job sender to send");
        }
        Err(why) => {
            // tracing::warn!("RESPONSE: {:?}", why);
        }
    }
}
