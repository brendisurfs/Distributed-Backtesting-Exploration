use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use flume::TryRecvError;
use hello_world::processor_client::ProcessorClient;
use hello_world::{Job, JobsReply, JobsRequest, StatusRequest, WorkerStatus};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use tonic::codec::CompressionEncoding;

pub mod hello_world {
    tonic::include_proto!("backtesting");
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

static PROC_FLAG: OnceLock<AtomicBool> = OnceLock::new();
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    PROC_FLAG.set(AtomicBool::new(false)).unwrap();
    let cpu_count = num_cpus::get() / 2;

    let (job_sender, job_recv) = flume::bounded::<JobsReply>(128);
    let (complet_send, complete_recv) = flume::bounded::<Job>(128);

    std::thread::spawn(move || loop {
        if let Ok(jobs_reply) = job_recv.try_recv() {
            let proc_flag = PROC_FLAG.get().unwrap();
            proc_flag.store(true, Ordering::SeqCst);
            let is_proc = proc_flag.load(Ordering::SeqCst);
            tracing::info!("IsProc: {is_proc}");

            // simulate work
            for (idx, job) in jobs_reply.jobs.iter().enumerate() {
                tracing::info!(idx, "Processing job {}", job.id);
                thread::sleep(ms(1000));
            }
            proc_flag.store(false, Ordering::SeqCst);
        }
        thread::sleep(ms(250));
    });

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let mut client = ProcessorClient::connect("http://[::1]:50051")
                .await
                .expect("client")
                .accept_compressed(CompressionEncoding::Gzip);

            // repeatedly call the server to let it know we want jobs.
            let mut call_tick = IntervalStream::new(interval(ms(250)));

            while call_tick.next().await.is_some() {
                let processing_flag = PROC_FLAG.get().unwrap();
                let is_proc = processing_flag.load(Ordering::SeqCst);

                if is_proc {
                    let status = StatusRequest {
                        status: WorkerStatus::Running.into(),
                    };
                    client.send_status(status).await.unwrap();
                    continue;
                }

                let req = StatusRequest {
                    status: WorkerStatus::Idle.into(),
                };

                client.send_status(req).await.unwrap();

                let request = tonic::Request::new(JobsRequest {
                    cores: cpu_count as i32,
                });

                let response = client.request_jobs(request).await;
                match response {
                    Ok(res) => {
                        job_sender
                            .send_async(res.into_inner())
                            .await
                            .expect("job sender to send");
                    }
                    Err(why) => {
                        // tracing::warn!("RESPONSE: {:?}", why);
                    }
                }
            }
        });
    Ok(())
}
