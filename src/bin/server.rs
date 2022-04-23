use std::{fmt::format, net::SocketAddr};

use nice::pb::{
    hello_service_server::{HelloService, HelloServiceServer},
    HelloReq, HelloRes,
};
use tokio::runtime::Builder;
use tonic::transport::Server;

fn main() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();

    rt.block_on(async {
        let addr: SocketAddr = "0.0.0.0:7788".parse().unwrap();

        let svc = HelloServiceServer::new(HelloServiceImpl {});

        Server::builder()
            .add_service(svc)
            .serve(addr)
            .await
            .unwrap();
    })
}

struct HelloServiceImpl;

#[async_trait::async_trait]
impl HelloService for HelloServiceImpl {
    async fn hello(
        &self,
        request: tonic::Request<HelloReq>,
    ) -> Result<tonic::Response<HelloRes>, tonic::Status> {
        let rbody = request.into_inner().body;
        let hostname: String = gethostname::gethostname().to_str().unwrap().into();
        let body = format!(">>> {}  {}", rbody, hostname);
        Ok(tonic::Response::new(HelloRes { body }))
    }
}
