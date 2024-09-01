mod handlers;
mod process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use handlers::{handle_job, send_status};
use hello_world::processor_client::ProcessorClient;
use hello_world::{CompleteRequest, Job, JobsReply, JobsRequest, StatusRequest, WorkerStatus};
use process::process_incoming_job;
use tokio::select;
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
static CONNECTED: OnceLock<AtomicBool> = OnceLock::new();

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    PROC_FLAG.set(AtomicBool::new(false)).unwrap();
    CONNECTED.set(AtomicBool::new(false)).unwrap();

    let cpu_count = num_cpus::get() / 2;

    let (reply_send, reply_recv) = flume::bounded::<JobsReply>(128);
    let (complete_send, complete_recv) = flume::bounded::<String>(128);

    std::thread::spawn(move || loop {
        while let Ok(jobs_reply) = reply_recv.recv() {
            process_incoming_job(jobs_reply, complete_send.clone());
        }
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

            let con_flag = CONNECTED.get().unwrap();
            con_flag.store(true, Ordering::SeqCst);

            // repeatedly call the server to let it know we want jobs.
            let mut call_tick = IntervalStream::new(interval(ms(250)));
            let mut status_tick = IntervalStream::new(interval(ms(1000)));

            loop {
                select! {
                    Some(_) = status_tick.next() => {
                        send_status(&mut client).await;
                    }
                    Some(_) = call_tick.next() => {
                        handle_job(&mut client, reply_send.clone()).await;
                    }

                    Ok(msg) = complete_recv.recv_async() => {
                        tracing::info!("Completed: {msg}");
                        client.complete_job(CompleteRequest{id: msg.clone(), data: msg}).await.unwrap();
                    }
                }
            }
        });
    Ok(())
}
