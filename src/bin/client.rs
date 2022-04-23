use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Sub;
use std::str::FromStr;
use std::sync::Arc;

use crossfire::mpsc;
use nice::pb::{hello_service_client::HelloServiceClient, HelloReq};
use std::net::*;
use std::thread;
use tokio::runtime::Builder;
use tokio::time;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tower::discover::Change;
use trust_dns_resolver::config::*;
use trust_dns_resolver::Resolver;

fn get_uris() -> HashSet<String> {
    let resolver = Resolver::from_system_conf().unwrap();
    let response = resolver
        .lookup_ip("hello.default.svc.cluster.local.")
        .unwrap();

    let uris: HashSet<String> = response
        .iter()
        .map(|a| format!("http://{}:7788", a))
        .collect();
    uris
}

fn main() {
    let (tx, rx) = mpsc::bounded_tx_blocking_rx_future(1);

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
        }
    });

    let rt = Arc::new(Builder::new_multi_thread().enable_all().build().unwrap());

    rt.clone().block_on(async move {
        let (ch, sender) = Channel::balance_channel(1024);

        rt.spawn(async move {
            let mut old: HashSet<String> = HashSet::new();

            loop {
                let new = rx.recv().await.unwrap();
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

        for i in 0..10000000 {
            send_request(ch.clone(), i).await;
        }
    })
}

async fn send_request(
    ch: Channel,
    i: i32,
) {
    let mut client = HelloServiceClient::new(ch);
    let body = format!("Alex{}", i);
    let req = HelloReq { body };
    let res = client.hello(req).await.unwrap().into_inner();
    println!(">>> {res:?}");
}
