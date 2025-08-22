use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};

mod cnf;

#[tokio::main]
async fn main() {
    let client = Client::try_default().await.unwrap();
    let pods: Api<Pod> = Api::default_namespaced(client);
}
