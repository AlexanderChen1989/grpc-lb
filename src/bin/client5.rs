use log::*;
use nice::pb::hello_service_client::HelloServiceClient;
use nice::pb::HelloReq;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use rand::thread_rng;
use std::future::Future;
use std::pin::Pin;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::thread;
use tokio::runtime::{Builder, Runtime};
// use tokio::sync::RwLock;
use std::sync::RwLock;
use tokio::time;
use tonic::transport::{self, Channel};
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
            sleep(1000).await;
            sender
                .send_async(Change::Insert(addr.into(), Endpoint::from_static(addr)))
                .await
                .unwrap();
        }
        sleep(10000).await;
        // sender
        //     .send_async(Change::Remove("http://localhost:8888".into()))
        //     .await
        //     .unwrap();
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

type Latency = Arc<AtomicU64>;

struct Chans {
    chans: Vec<(String, Channel, Latency)>,
    index: WeightedIndex<f32>,
}

impl Chans {
    fn new() -> Self {
        let chans = Vec::new();
        // weights cant by empty!
        let weights = [1.0];
        let index = WeightedIndex::new(&weights).unwrap();

        Self { chans, index }
    }

    fn add_chan(
        &mut self,
        key: String,
        chan: Channel,
    ) {
        self.chans
            .push((key, chan, Arc::new(AtomicU64::new(1))));
        self.update_index();
    }

    fn remove_chan(
        &mut self,
        key: String,
    ) {
        self.chans.retain(|(k, _, _)| *k != key);
        self.update_index();
    }

    fn update_index(&mut self) {
        let weights: Vec<f32> = self
            .chans
            .iter()
            .map(|(_, _, latency)| latency.load(Ordering::Relaxed))
            .map(|latency| 1.0 / (latency as f32))
            .collect();
        println!("{:?}", weights);
       
        self.index = WeightedIndex::new(&weights).unwrap();
    }

    fn pick_chan(&self) -> Option<(Channel, Arc<AtomicU64>)> {
        if self.chans.is_empty() {
            return None;
        }
        let mut rng = thread_rng();
        let idx = self.index.sample(&mut rng);
        let &(_, ref ch, ref latency) = self.chans.get(idx)?;
        return Some((ch.clone(), latency.clone()));
    }
}


#[derive(Clone)]
struct HelloChannel {
    rt: Arc<Runtime>,
    change_rx: flume::Receiver<Change>,

    chan_tx: flume::Sender<(Channel, Latency)>,
    chan_rx: flume::Receiver<(Channel, Latency)>,

    chans: Arc<RwLock<Chans>>,
}


impl HelloChannel {
    fn new(rt: Arc<Runtime>) -> (Self, flume::Sender<Change>) {
        let (change_tx, change_rx) = flume::bounded(10);
        let (chan_tx, chan_rx) = flume::bounded(10);
        let chans = Arc::new(RwLock::new(Chans::new()));

        let hc = Self {
            rt,
            change_rx,
            chan_tx,
            chan_rx,
            chans,
        };
        hc.start_change_loop();
        hc.start_index_update_loop();
        (hc, change_tx)
    }

    fn connect(
        rt: &Arc<Runtime>,
        endpoint: Endpoint,
    ) -> Result<Channel, transport::Error> {
        let (tx, rx) = flume::bounded(1);
        rt.spawn(async move {
            let res = endpoint.connect().await;
            tx.send_async(res).await.unwrap();
        });
        rx.recv().unwrap()
    }

    fn start_index_update_loop(&self) {
        let chans = self.chans.clone();

        thread::spawn(move || loop {
            thread::sleep(time::Duration::from_secs(2));
            chans.write().unwrap().update_index();
        });
    }

    fn start_change_loop(&self) {
        let HelloChannel {
            rt,
            change_rx,
            chans,
            ..
        } = self.clone();

        thread::spawn(move || {
            while let Ok(change) = change_rx.recv() {
                match change {
                    Change::Insert(key, endpoint) => match HelloChannel::connect(&rt, endpoint) {
                        Ok(chan) => {
                            chans.write().unwrap().add_chan(key, chan);
                        }
                        Err(e) => {
                            error!("{:?}", e);
                        }
                    },
                    Change::Remove(key) => {
                        chans.write().unwrap().remove_chan(key);
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
        match self.chans.read().unwrap().pick_chan() {
            Some((mut ch, latency)) => {
                let res = ch.poll_ready(cx);
                self.chan_tx.send((ch, latency)).unwrap();
                return res;
            }
            None => {
                let waker = cx.waker().clone();
                self.rt.spawn(async move {
                    sleep(10).await;
                    waker.wake();
                });
                return Poll::Pending;
            }
        }
    }

    fn call(
        &mut self,
        req: http::Request<BoxBody>,
    ) -> Self::Future {
        let chan_rx = self.chan_rx.clone();

        let fut = async move {
            let (mut chan, latency) = chan_rx.recv_async().await.unwrap();
            let now = time::Instant::now();
            let res = chan.call(req).await;
            let d = now.elapsed();
            latency.store(now.elapsed().as_millis() as u64, Ordering::Relaxed);
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
