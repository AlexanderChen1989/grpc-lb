use rand::Rng;
use std::{
    future::Future,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
};

use nice::pb::{
    hello_service_server::{HelloService, HelloServiceServer},
    HelloReq, HelloRes,
};
use tokio::runtime::{Builder, Runtime};
use tokio::time;

use tonic::transport::Server;

fn main() {
    let (rng_tx, rng_rx) = flume::bounded(0);

    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            let d: u64 = rng.gen_range(500..2000);

            rng_tx.send(d).unwrap();
        }
    });

    let rt = Builder::new_multi_thread().enable_all().build().unwrap();
    let rt = Arc::new(rt);

    rt.clone().block_on(async move {
        let addr: SocketAddr = "0.0.0.0:7788".parse().unwrap();

        let (req_tx, req_rx) = flume::bounded(1024);

        for _ in 0..10 {
            let w = Worker::new(&rt, &req_rx, &rng_rx);
            w.start();
        }

        let svc = HelloServiceImpl { rt, req_tx };

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
    rt: Arc<Runtime>,
    req_tx: flume::Sender<Request>,
}

fn after(n: u64) -> impl Future {
    time::sleep(time::Duration::from_millis(n))
}

#[async_trait::async_trait]
impl HelloService for HelloServiceImpl {
    async fn hello(
        &self,
        req: tonic::Request<HelloReq>,
    ) -> Result<tonic::Response<HelloRes>, tonic::Status> {
        let (res_tx, res_rx) = flume::bounded(1);
        let req = Request {
            req: req.into_inner(),
            res_tx,
        };
        self.req_tx.send_async(req).await.unwrap();
        let res = res_rx.recv_async().await.unwrap();
        Ok(tonic::Response::new(res))
    }
}

struct Worker {
    idx: usize,
    req_rx: flume::Receiver<Request>,
    rng_rx: flume::Receiver<u64>,
    rt: Arc<Runtime>,
}

static IDX: AtomicUsize = AtomicUsize::new(0);

impl Worker {
    fn new(
        rt: &Arc<Runtime>,
        req_rx: &flume::Receiver<Request>,
        rng_rx: &flume::Receiver<u64>,
    ) -> Self {
        let idx = IDX.fetch_add(1, Ordering::Relaxed);
        let req_rx = req_rx.clone();
        let rng_rx = rng_rx.clone();
        let rt = rt.clone();
        Self {
            idx,
            req_rx,
            rng_rx,
            rt,
        }
    }

    fn start(&self) {
        let idx = self.idx;
        let req_rx = self.req_rx.clone();
        self.rt.spawn(async move {
            while let Ok(Request { req: _, res_tx }) = req_rx.recv_async().await {
                let res = HelloRes {
                    body: format!("worker-{idx}"),
                };
                res_tx.send_async(res).await.unwrap();
                let d = time::Duration::from_millis(idx as u64 * 2);
                time::sleep(d).await;
            }
        });
    }
}
