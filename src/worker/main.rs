mod handlers;
mod process;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use handlers::{handle_job, send_status};
use backtesting::processor_client::ProcessorClient;
use backtesting::{CompleteRequest, JobsReply};
use process::process_incoming_job;
use tokio::select;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt;
use tonic::codec::CompressionEncoding;

pub use parallel_backtest::backtesting;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

static PROC_FLAG: OnceLock<AtomicBool> = OnceLock::new();
static CONNECTED: OnceLock<AtomicBool> = OnceLock::new();

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    PROC_FLAG.set(AtomicBool::new(false)).expect("PROC_FLAG to set");
    CONNECTED.set(AtomicBool::new(false)).expect("CONNECTED flag to be set");

    let (reply_send, reply_recv) = flume::bounded::<JobsReply>(128);
    let (complete_send, complete_recv) = flume::bounded::<String>(128);

    // Spawn a processor in a background OS thread.
    // Jobs in this project are set up to be compute heavy,
    // thus async threads are not the right decision to run in.
    std::thread::spawn(move || loop {
        while let Ok(jobs_reply) = reply_recv.recv() {
            process_incoming_job(jobs_reply, complete_send.clone());
        }
    });

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build().expect("tokio runtime to build")
        .block_on(async move {
            let mut client  = match ProcessorClient::connect("http://[::1]:50051").await {
                    Ok(client) => client.accept_compressed(CompressionEncoding::Gzip),
                    Err(why) => { 
                        if let Some(source) = why.source() {
                            tracing::error!("Unable to connect to server: {:?}", source.source());
                        }
                        return;
                    },
                };

            tracing::info!("connected to server");

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
