use log::*;
use nice::pb::hello_service_client::HelloServiceClient;
use nice::pb::HelloReq;
use std::collections::{HashSet};
use std::future::Future;
use std::pin::Pin;

use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::RwLock;
use tokio::time;
use tonic::transport::Channel;
use tonic::{body::BoxBody, codegen::http, transport::Endpoint};
use tower::Service;

fn main() {
    let _s = String::from("Hello, world");
    let rt = Arc::new(
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );

    let (ch, sender) = HelloChannel::new(rt.clone());
    rt.spawn(async move {
        for addr in ["http://localhost:7777", "http://localhost:8888"] {
            sleep(3000).await;
            sender
                .send_async(Change::Insert(addr.into(), Endpoint::from_static(addr)))
                .await
                .unwrap();
        }
        sleep(3000).await;
        sender
            .send_async(Change::Remove("http://localhost:8888".into()))
            .await
            .unwrap();
    });

    rt.block_on(async {
        let mut client = HelloServiceClient::new(ch);

        for _ in 0..1000000 {
            let req = HelloReq { body: "".into() };
            let res = client.hello(req).await;
            match res {
                Ok(res) => {
                    println!(">>> {:?}", res.into_inner().body);
                }
                Err(e) => {
                    println!("{e:?}")
                }
            }
            sleep(10).await;
        }
    });
}

#[derive(Clone)]
struct KeyChannel {
    key: String,
    chan: Channel,
}

#[derive(Clone)]
struct HelloChannel {
    rt: Arc<Runtime>,
    change_rx: flume::Receiver<Change>,

    keys: Arc<RwLock<HashSet<String>>>,
    chan_tx: flume::Sender<KeyChannel>,
    chan_rx: flume::Receiver<KeyChannel>,

    ready_chan_tx: flume::Sender<KeyChannel>,
    ready_chan_rx: flume::Receiver<KeyChannel>,
}

impl HelloChannel {
    fn new(rt: Arc<Runtime>) -> (Self, flume::Sender<Change>) {
        let (change_tx, change_rx) = flume::bounded(10);

        let keys = Arc::new(RwLock::new(HashSet::new()));
        let (chan_tx, chan_rx) = flume::bounded(10);
        let (ready_chan_tx, ready_chan_rx) = flume::bounded(10);
        let hc = Self {
            rt,
            change_rx,
            keys,
            chan_tx,
            chan_rx,
            ready_chan_tx,
            ready_chan_rx,
        };
        hc.start();
        (hc, change_tx)
    }

    fn start(&self) {
        let HelloChannel {
            rt,
            change_rx,
            keys,
            chan_tx,
            ..
        } = self.clone();

        rt.clone().spawn(async move {
            while let Ok(change) = change_rx.recv_async().await {
                match change {
                    Change::Insert(key, endpoint) => match endpoint.connect().await {
                        Ok(chan) => {
                            keys.write().await.insert(key.clone());
                            for _ in 0..10 {
                                chan_tx
                                    .send_async(KeyChannel {
                                        key: key.clone(),
                                        chan: chan.clone(),
                                    })
                                    .await
                                    .unwrap();
                            }
                        }
                        Err(e) => {
                            error!("{:?}", e);
                        }
                    },
                    Change::Remove(key) => {
                        keys.write().await.remove(&key);
                    }
                }
            }
        });
    }
}

type HelloChannelResponse = <Channel as Service<http::Request<BoxBody>>>::Response;
type HelloChannelError = <Channel as Service<http::Request<BoxBody>>>::Error;

impl Service<http::Request<BoxBody>> for HelloChannel {
    type Response = HelloChannelResponse;
    type Error = HelloChannelError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let HelloChannel {
            rt,
            chan_tx,
            chan_rx,
            ready_chan_tx,
            ..
        } = self.clone();

        if let Ok(mut chan) = chan_rx.try_recv() {
            let res = chan.chan.poll_ready(cx);
            if let Poll::Ready(Ok(())) = res {
                rt.spawn(async move {
                    ready_chan_tx.send(chan).unwrap();
                });
            } else {
                rt.spawn(async move {
                    chan_tx.send(chan).unwrap();
                });
            }

            return res;
        } else {
            let waker = cx.waker().clone();
            rt.spawn(async move {
                sleep(10).await;
                waker.wake();
            });
            return Poll::Pending;
        }
    }

    fn call(
        &mut self,
        req: http::Request<BoxBody>,
    ) -> Self::Future {
        let HelloChannel {
            rt,
            keys,
            chan_tx,
            ready_chan_rx,
            ..
        } = self.clone();

        let fut = async move {
            let mut chan = ready_chan_rx.recv_async().await.unwrap();
            let res = chan.chan.call(req).await;
            rt.spawn(async move {
                if keys.read().await.contains(&chan.key) {
                    let _ = chan_tx.send_async(chan).await;
                }
            });

            res
        };

        Box::pin(fut)
    }
}

async fn sleep(d: u64) {
    time::sleep(time::Duration::from_millis(d)).await;
}

enum Change {
    Insert(String, Endpoint),
    Remove(String),
}
