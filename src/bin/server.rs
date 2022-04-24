use std::{
    future::Future,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use nice::pb::{
    hello_service_server::{HelloService, HelloServiceServer},
    HelloReq, HelloRes,
};
use tokio::runtime::{Builder, Runtime};
use tokio::time;
use tonic::transport::Server;

fn main() {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let rt = Arc::new(rt);

    rt.clone().block_on(async {
        let addr: SocketAddr = "0.0.0.0:7788".parse().unwrap();

        let (workers_tx, workers_rx) = flume::bounded(10);

        let svc = HelloServiceImpl {
            min: 1,
            max: 8,
            idx: Arc::new(AtomicUsize::new(0)),
            rt,
            num_workers: Arc::new(AtomicUsize::new(0)),
            workers_rx,
            workers_tx,
        };

        svc.start_print_num_workers();
        svc.start_remove_workers();

        let svc = HelloServiceServer::new(svc);

        Server::builder()
            .add_service(svc)
            .serve(addr)
            .await
            .unwrap();
    })
}

struct Request {
    req: HelloReq,
    res_tx: flume::Sender<HelloRes>,
}

#[derive(Clone)]
struct HelloServiceImpl {
    idx: Arc<AtomicUsize>,
    rt: Arc<Runtime>,
    min: usize,
    max: usize,
    num_workers: Arc<AtomicUsize>,
    workers_tx: flume::Sender<HelloServiceWorker>,
    workers_rx: flume::Receiver<HelloServiceWorker>,
}

fn after(n: u64) -> impl Future {
    time::sleep(time::Duration::from_millis(n))
}

impl HelloServiceImpl {
    async fn fetch_worker(&self) -> Result<HelloServiceWorker, String> {
        tokio::select! {
            worker  = self.workers_rx.recv_async() => {
                worker.map_err(|e| format!("{}",e))
            }
            _  = after(1) => {
                if self.num_workers.load(Ordering::Relaxed) >= self.max {
                    return Err("not enough workers!".into());
                }
                let idx = self.idx.fetch_add(1, Ordering::Relaxed);
                let worker = HelloServiceWorker::new(idx, self.rt.clone(), self.num_workers.clone());
                self.num_workers.fetch_add(1, Ordering::Relaxed);
                Ok(worker)
            }
        }
    }

    async fn remove_worker(&self) {
        tokio::select! {
            r = self.workers_rx.recv_async() => {
                if let Err(e) = r {
                    println!(">>> {e:?}");
                }
            }
            _ = after(1) => {}
        }
    }

    async fn return_worker(&self, worker: HelloServiceWorker) {
        tokio::select! {
            _ = self.workers_tx.send_async(worker) => {}
            _ = after(1) => {}
        }
    }

    fn start_remove_workers(&self) {
        let svc = self.clone();
        self.rt.spawn(async move {
            loop {
                if svc.num_workers.load(Ordering::Relaxed) > svc.min {
                    svc.remove_worker().await;
                }
                time::sleep(time::Duration::from_secs(2)).await;
            }
        });
    }

    fn start_print_num_workers(&self) {
        let num_workers = self.num_workers.clone();
        self.rt.spawn(async move {
            loop {
                println!(">>> num_workers {}", num_workers.load(Ordering::Relaxed));
                time::sleep(time::Duration::from_secs(3)).await;
            }
        });
    }
}

#[async_trait::async_trait]
impl HelloService for HelloServiceImpl {
    async fn hello(
        &self,
        request: tonic::Request<HelloReq>,
    ) -> Result<tonic::Response<HelloRes>, tonic::Status> {
        // fetch worker
        let worker = self.fetch_worker().await;
        match worker {
            Ok(worker) => {
                let (tx, rx) = flume::bounded(1);
                let req = Request {
                    req: request.into_inner(),
                    res_tx: tx,
                };
                worker.do_work(req).await;
                // return worker
                self.return_worker(worker).await;

                let res = rx.recv_async().await.unwrap();

                Ok(tonic::Response::new(res))
            }
            Err(e) => Err(tonic::Status::aborted(e)),
        }
    }
}

struct HelloServiceWorker {
    idx: usize,
    req_rx: flume::Receiver<Request>,
    req_tx: flume::Sender<Request>,
    rt: Arc<Runtime>,
    num_workers: Arc<AtomicUsize>,
}

impl Drop for HelloServiceWorker {
    fn drop(&mut self) {
        self.num_workers.fetch_sub(1, Ordering::Relaxed);
    }
}

impl HelloServiceWorker {
    fn new(idx: usize, rt: Arc<Runtime>, num_workers: Arc<AtomicUsize>) -> Self {
        let (req_tx, req_rx) = flume::bounded(1);
        let worker = Self {
            idx,
            req_tx,
            req_rx,
            rt,
            num_workers,
        };
        worker.start();
        worker.num_workers.fetch_add(1, Ordering::Relaxed);

        worker
    }

    async fn do_work(&self, req: Request) {
        self.req_tx.send_async(req).await.unwrap();
    }

    fn start(&self) {
        let idx = self.idx;
        let req_rx = self.req_rx.clone();

        self.rt.spawn(async move {
            while let Ok(req) = req_rx.recv_async().await {
                let body = format!(">>> worker{} => {}", idx, req.req.body);
                let res = HelloRes { body };
                time::sleep(time::Duration::from_secs(1)).await;
                let _ = req.res_tx.send_async(res).await;
            }
            println!(">>> worker {} stopped", idx);
        });
    }
}
