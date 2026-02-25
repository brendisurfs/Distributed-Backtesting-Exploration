# Distributed Backtesting Exploration

### Problem & Motivation

This project is an exploration into building a distributed backtesting system that could be run across multiple computers.
Backtesting strategies across thousands of stocks is embarrassingly parallel, but also CPU-intensive (building indicators, linear regressions, etc.). 
The idea of this system is to distribute that work across N machines with resource availability in mind.

It was inspired by how rendering farms distribute out work, which I have set up myself through SideFX Houdini and Blender with Flamenco, as well as being an early(ish) validator on the RNDR Network. 

### Design Decisions

I chose to use OS threads for tasks because backtesting is CPU-bound. 
Running backtesting processes on Tokio threads would block executor threads and starve I/O. 
I decided to only use current_thread() for the worker, as gRPC polling is minimal and I/O bound. Using the full multi-threaded runtime is wasteful. 

I chose gRPC for this project because Protobufs serve as an enforced, typed contract between server and workers, keeping them in sync from one source of truth. 
gRPC supports built-in compression, which significantly reduces payload size when transferring large OHLC CSV files as bytes.
Workers report their CPU count so the server can batch jobs proportionally, giving higher-core machines more work.

The server will prune workers that haven't checked in within 10 seconds, making peer health incredibly simple to reason about.
The system is based on polling, rather than streaming, to reduce initial complexity when building it out. 
This works for a small system like this, but would waste time eventually on a large scale operation, where streaming would be more appropriate.

### Protocol

#### RequestJobs 

RequestJobs is the RPC that gets called by a worker when it comes online. 
It sends its number of available cores to the server, and waits for a response of JobsReply, which has a job id and a file as bytes to process.

#### SendStatus

SendStatus is an RPC that tells the server if the worker is running on its job. 
This allows us to keep track of who is busy vs who can take new work.

#### CompleteJob

CompleteJob is the RPC called when a worker finishes its work. It sends the id of the job, and the processed data back to the server.


### Architecture 

  ┌─────────────────────────────────────────┐
  │              Server (Dispatcher)        │
  │                                         │
  │  files: Vec<String>  ──► jobs_completed │
  │  peers: HashMap<Addr, Peer>             │
  │  ┌─────────────────────────────────────┐│
  │  │  peer health check thread (100ms)   ││
  │  └─────────────────────────────────────┘│
  └────────────┬────────────────────────────┘
               │  gRPC / gzip (tonic)
      ┌────────┴────────┐
      │                 │
  ┌───▼───┐         ┌───▼───┐
  │Worker │   ...   │Worker │
  │                         │
  │ tokio current_thread    │
  │ ┌─────────────────────┐ │
  │ │ OS thread           │ │
  │ │ CPU-bound backtest  │ │
  │ └─────────────────────┘ │
  └─────────────────────────┘



### How to run

To run a server: 
```cargo r --release --bin server```


To run a worker on your local computer: 
```cargo r --release --bin worker```


### Limitations / Known Issues

This project is mostly an exploration, not a truly robust production system. 
There are a few obvious things that disqualify this project as "production-ready":
 
- The job state is stored in-memory with zero durability. If the server crashes/shuts down/loses power, the queue is lost. 

- No retry if a worker fails mid job.

- No backtesting strategies implemented in a demo environment. The system only sleeps, simulating work.

- Many values are hardcoded. In a real production system, I would have a CLI wrapper with proper arguments for node addresses and authentication. While I may do that in the future, currently that is out of the scope of experimentation.

- There are no robust instructions or logic for graceful shutdown. This is intentional for an exploration project, not a production system. 
