use std::{net::SocketAddr, sync::Arc};

use crate::fwd::{
    Forwarder,
    clients::ClientPool,
    proxy::{Proxy, ProxyDestination},
};
use anyhow::Result;
use clap::Args;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use tokio::net::TcpListener;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Args)]
pub struct ProxyCommandArguments {
    #[arg(short, long, help = "Namespace to use")]
    pub namespace: Option<String>,

    #[arg(short, long, help = "Local address to listen on")]
    pub bind: SocketAddr,

    #[arg(short, long, help = "Remote address to forward to")]
    pub target: SocketAddr,

    #[arg(short, long, help = "The kubeconfig context to use")]
    pub context: Option<String>,
}

pub async fn init(
    ProxyCommandArguments {
        bind: bind_addr,
        target,
        namespace,
        context,
        ..
    }: ProxyCommandArguments,
) -> Result<()> {
    let tracker = TaskTracker::new();
    let pool = ClientPool::default();
    let client = match context {
        Some(context) => pool.get_or_insert(&context).await?,
        None => pool.get_default().await?,
    };

    let token = CancellationToken::new();
    let namespace = namespace.as_deref().unwrap_or(client.default_namespace());
    let socket = TcpListener::bind(bind_addr).await?;
    let api = Api::<Pod>::namespaced(client.clone(), namespace);
    let api_ptr = Arc::new(api.clone());
    let proxy = Proxy::new(api);

    proxy.spawn(&ProxyDestination::Tcp(target)).await?;

    tracker.spawn(bind(
        api_ptr,
        proxy.get_name(),
        socket,
        token.child_token(),
        tracker.clone(),
    ));

    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {},
        _ = proxy.wait_until_exit() => {},
    }

    token.cancel();
    tracker.close();

    tracker.wait().await;
    proxy.abort().await?;

    Ok(())
}

pub async fn bind(
    api: Arc<Api<Pod>>,
    name: String,
    socket: TcpListener,
    token: CancellationToken,
    tracker: TaskTracker,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            () = token.cancelled() => break,
            Ok((connection, _)) = socket.accept() => {
                let api = api.clone();
                let token = token.child_token();

                tracker.spawn(Forwarder::upstream(api, Proxy::PORT, name.clone(), connection, token));
            }
        }
    }

    Ok(())
}
