use std::sync::Arc;

use tokio::runtime::Builder;

fn main() {
    let rt = Arc::new(Builder::new_multi_thread().enable_all().build().unwrap());

    rt.block_on(async move {
        // 1. GET
        let body = reqwest::get("http://localhost:3000/")
            .await.unwrap()
            .text()
            .await.unwrap();

        println!("body = {:?}", body);

       
    });
}
