use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Sub;
use std::str::FromStr;
use std::sync::Arc;

use std::net::*;
use trust_dns_resolver::config::*;
use trust_dns_resolver::Resolver;

use nice::pb::{hello_service_client::HelloServiceClient, HelloReq};
use tokio::runtime::Builder;
use tokio::time;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tower::discover::Change;

fn main() {
    let resolver = Resolver::from_system_conf().unwrap();
    let response = resolver.lookup_ip("www.baidu.com").unwrap();

    let new: HashSet<String> = response
        .iter()
        .map(|a| format!("http://{}:7788", a))
        .collect();

	let mut old: HashSet<String> = HashSet::new();
	old.insert("http://180.97.34.96:7788".into());
	old.insert("http://180.97.34.86:7788".into());

	let added = new.sub(&old);
	let removed = old.sub(&new);

    println!(">>> added: {:?} removed: {:?}", added, removed);
}
