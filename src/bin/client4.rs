use log::*;
use nice::pb::hello_service_client::HelloServiceClient;
use nice::pb::HelloReq;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::RwLock;
use tokio::time;
use tonic::transport::Channel;
use tonic::{body::BoxBody, codegen::http, transport::Endpoint};
use tower::Service;

fn main() {
    let rt = Arc::new(
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );

    let (ch, sender) = HelloChannel::new(rt.clone());
    rt.spawn(async move {
        for addr in ["http://localhost:7788", "http://localhost:7789"] {
            sleep(3000).await;
            sender
                .send_async(Change::Insert(addr.into(), Endpoint::from_static(addr)))
                .await
                .unwrap();
        }
        sleep(3000).await;
        sender
            .send_async(Change::Remove("http://localhost:7788".into()))
            .await
            .unwrap();
    });

    rt.block_on(async {
        let mut client = HelloServiceClient::new(ch);

        loop {
            let req = HelloReq { body: "".into() };
            let res = client.hello(req).await;
            match res {
                Ok(res) => {
                    println!("{:?}", res.into_inner().body)
                }
                Err(e) => {
                    println!("{e:?}")
                }
            }
            sleep(100).await;
        }
    });
}

type HelloChannelResponse = <Channel as Service<http::Request<BoxBody>>>::Response;
type HelloChannelError = <Channel as Service<http::Request<BoxBody>>>::Error;

struct Request {
    req: http::Request<BoxBody>,
    res_tx: flume::Sender<Result<HelloChannelResponse, HelloChannelError>>,
}

#[derive(Clone)]
struct HelloChannel {
    rt: Arc<Runtime>,
    change_rx: flume::Receiver<Change<String>>,
    workers: Arc<RwLock<HashMap<String, flume::Sender<()>>>>,
    workers_num: Arc<AtomicUsize>,
    request_tx: flume::Sender<Request>,
    request_rx: flume::Receiver<Request>,
    waker_tx: flume::Sender<Waker>,
    waker_rx: flume::Receiver<Waker>,
}

impl HelloChannel {
    fn new(rt: Arc<Runtime>) -> (Self, flume::Sender<Change<String>>) {
        let (change_tx, change_rx) = flume::bounded(10);
        let workers = Arc::new(RwLock::new(HashMap::new()));
        let workers_num = Arc::new(AtomicUsize::new(0));
        let (request_tx, request_rx) = flume::bounded(10);
        let (waker_tx, waker_rx) = flume::bounded(10);

        let hc = Self {
            rt,
            change_rx,
            workers,
            workers_num,
            request_tx,
            request_rx,
            waker_tx,
            waker_rx,
        };
        hc.start();
        (hc, change_tx)
    }

    fn start(&self) {
        self.clone().start_change_loop();
    }

    fn handle_request(
        rt: Arc<Runtime>,
        mut chan: Channel,
        req: Request,
    ) {
        let Request { req, res_tx } = req;

        rt.spawn(async move {
            let res = chan.call(req).await;
            res_tx.send_async(res).await.unwrap();
        });
    }

    fn remove_workers(
        self,
        key: String,
    ) {
        let HelloChannel { rt, workers, .. } = self;
        rt.spawn(async move {
            let mut workers = workers.write().await;
            workers.remove_entry(&key);
        });
    }

    fn start_workers(
        self,
        key: String,
        chan: Channel,
    ) {
        let HelloChannel {
            rt,
            workers,
            request_rx,
            workers_num,
            waker_rx,
            ..
        } = self;
        let (stop_tx, stop_rx) = flume::bounded::<()>(1);

        for _ in 0..10 {
            let rt = rt.clone();
            let stop_rx = stop_rx.clone();
            let chan = chan.clone();
            let request_rx = request_rx.clone();
            let workers_num = workers_num.clone();
            rt.clone().spawn(async move {
                workers_num.fetch_add(1, Ordering::Relaxed);
                loop {
                    tokio::select! {
                        _ = stop_rx.recv_async() => {  break; }
                        req = request_rx.recv_async() => {
                            match req {
                                Ok(req) => {
                                    HelloChannel::handle_request(rt.clone(), chan.clone(), req)
                                },
                                Err(e) => error!("{:?}", e),
                            }
                        }
                    }
                }
                workers_num.fetch_sub(1, Ordering::Relaxed);
            });
        }

        rt.spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(10) => {break;}
                    res = waker_rx.recv_async() => {
                        match res {
                            Ok(waker) => waker.wake(),
                            Err(e) => error!("{:?}", e),
                        }
                    }
                }
            }
        });

        rt.spawn(async move {
            let mut workers = workers.write().await;
            workers.insert(key, stop_tx);
        });
    }

    fn start_change_loop(&self) {
        let HelloChannel { rt, change_rx, .. } = self.clone();
        let hello_channel = self.clone();

        rt.clone().spawn(async move {
            while let Ok(change) = change_rx.recv_async().await {
                match change {
                    Change::Insert(key, endpoint) => match endpoint.connect().await {
                        Ok(chan) => {
                            HelloChannel::start_workers(hello_channel.clone(), key, chan);
                        }
                        Err(_) => todo!(),
                    },
                    Change::Remove(key) => {
                        HelloChannel::remove_workers(hello_channel.clone(), key);
                    }
                }
            }
        });
    }
}

impl Service<http::Request<BoxBody>> for HelloChannel {
    type Response = HelloChannelResponse;
    type Error = HelloChannelError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if self.workers_num.load(Ordering::Relaxed) == 0 {
            self.waker_tx.send(cx.waker().clone()).unwrap();
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(
        &mut self,
        req: http::Request<BoxBody>,
    ) -> Self::Future {
        let req_tx = self.request_tx.clone();
        let (res_tx, res_rx) = flume::bounded(1);
        let req = Request { req, res_tx };

        let fut = async move {
            req_tx.send_async(req).await.unwrap();
            res_rx.recv_async().await.unwrap()
        };

        Box::pin(fut)
    }
}

async fn sleep(d: u64) {
    time::sleep(time::Duration::from_millis(d)).await;
}

enum Change<K: PartialEq> {
    Insert(K, Endpoint),
    Remove(K),
}
