use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::time;

fn main() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    let rt = Arc::new(rt);

    rt.clone().block_on(async {
        let (tx, rx) = flume::bounded(0);
        let (done_tx, done_rx) = flume::bounded(10);
        for id in 0..10 {
            let done_tx = done_tx.clone();
            let tx = tx.clone();
            rt.spawn(async move {
                for i in 0..10000 {
                    tx.send_async((id, i)).await.unwrap();
                    time::sleep(time::Duration::from_secs(1)).await;
                }

                done_tx.send_async(()).await.unwrap();
            });
        }
        drop(tx);
        drop(done_tx);

        for id in 0..10 {
            let rt = rt.clone();
            let rx = rx.clone();
            rt.spawn(async move {
                while let Ok(msg) = rx.recv_async().await {
                    println!(">>> worker {id} => {msg:?}");
                }
            });
        }

        while let Ok(_) = done_rx.recv_async().await {}
    });
}
