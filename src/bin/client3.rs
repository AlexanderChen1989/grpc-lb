use nice::pb::hello_service_client::HelloServiceClient;
use nice::pb::HelloReq;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll, Waker};
use std::thread;
use tokio::runtime::{Builder, Runtime};
use tokio::time;
use tonic::transport::Channel;
use tonic::{
    body::BoxBody,
    codegen::http,
    transport::{self, Endpoint},
};
use tower::Service;

fn main() {
    let rt = Arc::new(
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    );

    let (ch, sender) = MyChannel::<String>::new(rt.clone());
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

async fn sleep(d: u64) {
    time::sleep(time::Duration::from_millis(d)).await;
}

enum Change<K: PartialEq> {
    Insert(K, Endpoint),
    Remove(K),
}

struct MyChannel<K: PartialEq> {
    rt: Arc<Runtime>,
    channels: Arc<RwLock<Vec<(K, Channel)>>>,
    counter: AtomicUsize,

    ch_tx: flume::Sender<Channel>,
    ch_rx: flume::Receiver<Channel>,

    change_rx: flume::Receiver<Change<K>>,
    waker_tx: flume::Sender<Waker>,
    waker_rx: flume::Receiver<Waker>,
}

struct Pair<K: PartialEq>(K, Endpoint);

impl<K: PartialEq> PartialEq for Pair<K> {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.0 == other.0
    }
}

fn connect(
    rt: Arc<Runtime>,
    endpoint: Endpoint,
) -> Result<Channel, transport::Error> {
    let (tx, rx) = flume::bounded(1);

    rt.spawn(async move {
        let res = endpoint.connect().await;
        tx.send_async(res).await.unwrap();
    });

    rx.recv().unwrap()
}

impl<K: PartialEq + Send + Sync + 'static> MyChannel<K> {
    fn new(rt: Arc<Runtime>) -> (Self, flume::Sender<Change<K>>) {
        let (change_tx, change_rx) = flume::bounded(10);
        let (ch_tx, ch_rx) = flume::bounded(10);
        let (waker_tx, waker_rx) = flume::bounded(10);

        let channels: Arc<RwLock<Vec<(K, Channel)>>> = Arc::new(RwLock::new(vec![]));
        let counter = AtomicUsize::new(0);

        let ch = MyChannel {
            rt,
            change_rx,
            ch_tx,
            ch_rx,
            counter,
            channels,
            waker_rx,
            waker_tx,
        };
        ch.start();

        (ch, change_tx)
    }

    fn start(&self) {
        let rt = self.rt.clone();
        let change_rx = self.change_rx.clone();
        let chs = self.channels.clone();
        let waker_rx = self.waker_rx.clone();

        thread::spawn(move || {
            while let Ok(change) = change_rx.recv() {
                match change {
                    Change::Insert(k, endpoint) => {
                        let res = connect(rt.clone(), endpoint);
                        match res {
                            Ok(ch) => {
                                let mut chs = chs.write().unwrap();
                                chs.push((k, ch));
                                while let Ok(waker) =
                                    waker_rx.recv_timeout(time::Duration::from_millis(1))
                                {
                                    waker.wake();
                                }
                            }
                            Err(e) => {
                                println!("{e:?}");
                            }
                        }
                    }
                    Change::Remove(key) => {
                        let mut chs = chs.write().unwrap();
                        let old_chs = std::mem::take(&mut *chs);
                        *chs = old_chs.into_iter().filter(|(k, _)| *k != key).collect();
                    }
                }
            }
        });
    }
}

impl<K: PartialEq> Service<http::Request<BoxBody>> for MyChannel<K> {
    type Response = <Channel as Service<http::Request<BoxBody>>>::Response;
    type Error = <Channel as Service<http::Request<BoxBody>>>::Error;
    type Future = <Channel as Service<http::Request<BoxBody>>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        let mut ch = {
            let chs = self.channels.read().unwrap();
            if chs.len() == 0 {
                let waker = cx.waker().clone();
                self.waker_tx.send(waker).unwrap();
                return Poll::Pending;
            }
            let idx = self.counter.fetch_add(1, Ordering::Relaxed) % chs.len();
            chs[idx].1.clone()
        };

        match ch.poll_ready(cx) {
            Poll::Ready(_) => {
                self.ch_tx.send(ch).unwrap();
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn call(
        &mut self,
        request: http::Request<BoxBody>,
    ) -> Self::Future {
        let mut ch = self.ch_rx.recv().unwrap();

        ch.call(request)
    }
}
