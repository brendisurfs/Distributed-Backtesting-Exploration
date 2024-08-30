use std::{
    collections::HashMap,
    default, fs,
    net::SocketAddr,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use hello_world::{
    processor_server::{Processor, ProcessorServer},
    Job, JobsReply, JobsRequest, StatusReply, StatusRequest, WorkerStatus,
};
use time::OffsetDateTime;
use tonic::{codec::CompressionEncoding, transport::Server, Code, Request, Response, Status};
use tracing::{debug, info, warn};
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;

pub mod hello_world {
    tonic::include_proto!("backtesting");
}

#[derive(Debug, Clone)]
struct Peer {
    addr: SocketAddr,
    status: WorkerStatus,
    last_connection: i64,
}

#[derive(Debug)]
pub struct Dispatcher {
    // the files we need to send out for processing.
    files: Mutex<Vec<String>>,
    // keeps track of our connected peers.
    peers: Arc<Mutex<HashMap<SocketAddr, Peer>>>,
    // keeps track of jobs we have completed, and if we need to send more out.
    jobs_completed: Mutex<HashMap<String, bool>>,
}
impl Dispatcher {
    pub fn new(paths: Vec<String>) -> Self {
        let peers = Arc::new(Mutex::new(HashMap::<SocketAddr, Peer>::new()));

        // Spawn a new task to check our peers.
        let peers_to_check = peers.clone();
        std::thread::spawn(move || loop {
            {
                let mut lock = peers_to_check.lock().unwrap();
                for (addr, peer) in lock.clone().into_iter() {
                    if did_fail_checkin(peer.last_connection) {
                        tracing::info!("Removing addr {addr:?}");
                        lock.remove(&addr);
                    }
                }
            };

            std::thread::sleep(Duration::from_secs(1));
        });

        let jobs_completed = Mutex::new(HashMap::new());

        Self {
            peers,
            jobs_completed,
            files: Mutex::new(paths),
        }
    }
}

#[tonic::async_trait]
impl Processor for Dispatcher {
    // receive the status from a worker
    async fn send_status(
        &self,
        req: Request<StatusRequest>,
    ) -> Result<Response<StatusReply>, Status> {
        let addr = req.remote_addr();
        let status = req.into_inner().status();

        let prev_status = {
            let lock = self.peers.lock().unwrap();
            let peer = lock.get(&addr.unwrap()).unwrap();
            peer.status
        };
        if prev_status != status {
            tracing::info!("Status Update: addr={addr:?} status={status:?}");
        }

        Ok(Response::new(StatusReply {}))
    }

    // A worker requested a job
    async fn request_jobs(
        &self,
        request: Request<JobsRequest>,
    ) -> Result<Response<JobsReply>, Status> {
        let addr = request.local_addr();
        let num_cores = request.into_inner().cores;

        if let Some(addr) = addr {
            let mut peers = self.peers.lock().unwrap();
            let last_connection = time::OffsetDateTime::now_utc().unix_timestamp();

            let new_peer = Peer {
                status: WorkerStatus::Idle,
                last_connection,
                addr,
            };

            let last_insert = peers.insert(addr, new_peer);
            if last_insert.is_none() {
                tracing::info!("New Peer added -> {addr:?} cores: {num_cores}");
            }
        }

        let files = {
            let mut lock = self.files.lock().unwrap();
            split_off_n_jobs(&mut lock, num_cores as usize)
        };

        if let Some(some_files) = files {
            let jobs = convert_files_to_jobs(some_files);
            info!("Num jobs: {}", jobs.len());
            let res = JobsReply { jobs };
            return Ok(Response::new(res));
        }
        return Err(Status::new(Code::Ok, "No more jobs available"));
    }
}

// splits off n jobs from our store.
fn split_off_n_jobs(lock: &mut MutexGuard<'_, Vec<String>>, n_jobs: usize) -> Option<Vec<String>> {
    let len_of_files = lock.len();
    if lock.is_empty() {
        return None;
    }

    if n_jobs >= len_of_files {
        return Some(lock.drain(..len_of_files).collect::<Vec<_>>());
    }

    Some(lock.split_off(n_jobs))
}

fn convert_files_to_jobs(files: Vec<String>) -> Vec<Job> {
    files
        .iter()
        .map(|p| {
            let id = Uuid::new_v4().to_string();
            let file = std::fs::read(p).unwrap();
            Job { id, file }
        })
        .collect::<Vec<_>>()
}

// checks if the peer is still active or not.
fn did_fail_checkin(timestamp: i64) -> bool {
    let now = OffsetDateTime::now_utc();
    let last_check = OffsetDateTime::from_unix_timestamp(timestamp).unwrap();

    let delta = now - last_check;

    delta.whole_minutes() > 1
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let addr = "[::1]:50051".parse()?;
    tracing::info!("server stared on [::1]:50051");

    let paths = [
        "/Users/brendi/Sync/OHLCData/Stocks/30min/CHWY_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/PTON_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/NVDA_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/CVNA_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/RIVN_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/LYFT_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/SG_full_30min_adjsplit.csv",
        "/Users/brendi/Sync/OHLCData/Stocks/30min/AAPL_full_30min_adjsplit.csv",
    ]
    .map(|p| p.to_string())
    .to_vec();

    let dispatcher = Dispatcher::new(paths);
    let service = ProcessorServer::new(dispatcher).send_compressed(CompressionEncoding::Gzip);
    Server::builder().add_service(service).serve(addr).await?;

    Ok(())
}
