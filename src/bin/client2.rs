use std::collections::HashMap;
use std::sync::Arc;

use nice::pb::{hello_service_client::HelloServiceClient, HelloReq};
use tokio::sync::RwLock;
use tokio::time;

#[tokio::main]
async fn main() {
    let mut client = HelloServiceClient::connect("http://localhost:7788")
        .await
        .unwrap();

    let counter = Arc::new(RwLock::new(HashMap::<String, usize>::new()));
    {
        let counter = counter.clone();
        tokio::spawn(async move {
            loop {
                time::sleep(time::Duration::from_secs(2)).await;
                println!(">>>>>>>>>>>");
                let mut counter = counter.write().await;
                let mut pairs = counter.iter().map(|e| e).collect::<Vec<_>>();
                pairs.sort_by(|a, b| b.1.cmp(a.1));
                for (k, v) in pairs {
                    println!("{k} => {v}")
                }
                counter.clear();
            }
        });
    }
    for _ in 0..100000 {
        let res = client
            .hello(HelloReq {
                body: "Hello".into(),
            })
            .await;

        match res {
            Ok(res) => {
                let key = res.into_inner().body;
                let mut counter = counter.write().await;
                let num = counter.entry(key).or_insert(0);
                *num += 1;
            }
            Err(e) => {
                println!(">>>> error {e:?}");
            }
        }
    }
}
