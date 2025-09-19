use std::{net::SocketAddr, sync::Arc};

use crate::fwd::{clients::ClientPool, forward, proxy::Proxy};
use anyhow::Result;
use clap::Args;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[derive(Args)]
pub struct ProxyCommandArguments {
    #[arg(short, long, help = "Namespace to use")]
    pub namespace: Option<String>,

    #[arg(short, long, help = "Local address to listen on")]
    pub bind: SocketAddr,

    #[arg(short, long, help = "Remote address to forward to")]
    pub target: SocketAddr,

    #[arg(short, long, default_value = "TCP", help = "Protocol to use")]
    pub protocol: String,

    #[arg(short, long, help = "The kubeconfig context to use")]
    pub context: Option<String>,
}

pub async fn init(
    ProxyCommandArguments {
        bind,
        target,
        protocol,
        namespace,
        context,
        ..
    }: ProxyCommandArguments,
) -> Result<()> {
    let pool = ClientPool::default();
    let client = match context {
        Some(context) => pool.get_or_insert(&context).await?,
        None => pool.get_default().await?,
    };

    let token = CancellationToken::new();
    let namespace = namespace.as_deref().unwrap_or(client.default_namespace());
    let socket = TcpListener::bind(bind).await?;
    let api = Api::<Pod>::namespaced(client.clone(), namespace);
    let api_ptr = Arc::new(api.clone());
    let proxy = Proxy::new(api);
    let name = format!("kubef-{}", proxy.id);

    // TODO: horrible hack
    let future = || {
        let token = token.clone();

        async move {
            while let Ok((connection, _)) = socket.accept().await {
                let api = api_ptr.clone();
                let token = token.child_token();

                tokio::spawn(forward(api, 8080, name.clone(), connection, token));
            }
        }
    };

    proxy.apply(bind.port(), &target, &protocol).await?;

    tokio::spawn(future());

    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => {},
        _ = proxy.wait_until_exit() => {},
    }

    token.cancel();
    proxy.delete().await?;

    Ok(())
}
