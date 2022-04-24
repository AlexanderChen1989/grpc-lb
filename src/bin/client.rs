use std::collections::HashSet;
use std::ops::Sub;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use nice::pb::{hello_service_client::HelloServiceClient, HelloReq};
use std::thread;
use tokio::runtime::Builder;
use tokio::time;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tower::discover::Change;
use trust_dns_resolver::Resolver;

fn main() {
    let (tx, rx) = flume::bounded(10);

    thread::spawn(move || {
        let resolver = Resolver::from_system_conf().unwrap();

        loop {
            let response = resolver
                .lookup_ip("hello.default.svc.cluster.local.")
                .unwrap();

            let uris: HashSet<String> = response
                .iter()
                .map(|a| format!("http://{}:7788", a))
                .collect();
            tx.send(uris).unwrap();

            thread::sleep(time::Duration::from_secs(2));
        }
    });

    let rt = Arc::new(Builder::new_multi_thread().enable_all().build().unwrap());

    rt.clone().block_on(async move {
        let (ch, sender) = Channel::balance_channel(1024);

        rt.spawn(async move {
            let mut old: HashSet<String> = HashSet::new();

            loop {
                let new = rx.recv_async().await.unwrap();
                let added = new.sub(&old);
                let removed = old.sub(&new);

                for uri in added {
                    let change = Change::Insert(uri.clone(), Endpoint::from_str(&uri).unwrap());
                    sender.send(change).await.unwrap();
                }

                for uri in removed {
                    let change = Change::Remove(uri);
                    sender.send(change).await.unwrap();
                }
                old = new;
            }
        });

        static ID: AtomicU64 = AtomicU64::new(0);

        for i in 0..20 {
            let ch = ch.clone();
            rt.spawn(async move {
                loop {
                    let id = ID.fetch_add(1, Ordering::Relaxed);
                    send_request(ch.clone(), id).await;
                    time::sleep(time::Duration::from_millis(20)).await;
                }
            });
        }

        time::sleep(time::Duration::from_secs(1000000)).await;
    })
}

async fn send_request(ch: Channel, i: u64) {
    let mut client = HelloServiceClient::new(ch);
    let body = format!("{}", i);
    let req = HelloReq { body };
    let res = client.hello(req).await.unwrap().into_inner();
    println!(">>> {res:?}");
}
