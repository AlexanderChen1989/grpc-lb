use std::thread;

use nice::pb::{hello_service_client::HelloServiceClient, HelloReq};
use rand::Rng;
use tokio::time;

#[tokio::main]
async fn main() {
    let client = HelloServiceClient::connect("http://localhost:7788")
        .await
        .unwrap();

    let (rng_tx, rng_rx) = flume::bounded(0);

    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        loop {
            let d: u64 = rng.gen_range(0..2000);

            rng_tx.send(d).unwrap();
        }
    });

    for _ in 0..10 {
        let mut client = client.clone();
        let rng_rx = rng_rx.clone();
        tokio::spawn(async move {
            for i in 0..100000 {
                let res = client
                    .hello(HelloReq {
                        body: format!("msg{i}"),
                    })
                    .await;

                match res {
                    Ok(_) => {
                        println!(">>>> ok");
                    }
                    Err(e) => {
                        println!(">>>> error {e:?}");
                    }
                }

                let d = rng_rx.recv_async().await.unwrap();
                time::sleep(time::Duration::from_millis(d)).await;
            }
        });
    }

    time::sleep(time::Duration::from_secs(10000)).await;
}
