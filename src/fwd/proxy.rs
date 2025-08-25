use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::{Context, Result};
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    Api, Client,
    api::PostParams,
    runtime::{conditions::is_pod_running, wait::await_condition},
};
use serde_json::json;
use tokio::{io::AsyncWriteExt, net::UdpSocket};
use tokio_util::{bytes::Buf, codec::BytesCodec, sync::CancellationToken, udp::UdpFramed};

use crate::{cnf::schema::Resource, env::MAX_CONCURRENT_CONNECTIONS};

pub async fn proxy(resource: Resource, client: Client, token: CancellationToken) -> Result<()> {
    let api = Api::<Pod>::namespaced(client, &resource.namespace.as_ref());

    let pod_name = format!("kubef-proxy-{}", resource.alias);
    let pod: Pod = serde_json::from_value(json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": pod_name.as_str(),
            "namespace": resource.namespace,
        },
        "spec": {
            "containers": [
                {
                    "name": "proxy",
                    "image": "alpine/socat",
                    "command": [
                        "socat",
                        "TCP-LISTEN:8080,reuseaddr,fork",
                        "UDP:localhost:8080", // TODO: add port from resource
                    ],
                }
            ],
        },
    }))?;

    api.create(&PostParams::default(), &pod).await?;

    tokio::select! {
        biased;
        _ = token.cancelled() => { },
        _ = tokio::time::sleep(Duration::from_secs(10)) => { },
        _ = await_condition(api, pod_name.as_str(), is_pod_running()) => {},
    };

    Ok(())
}

pub async fn bind(
    proxy_name: String,
    resource: Resource,
    api: Api<Pod>,
    token: CancellationToken,
) -> Result<()> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, resource.ports.local));
    let server = UdpSocket::bind(addr).await?;

    let mut forwarding = api
        .portforward(&proxy_name, &[resource.ports.remote])
        .await?;

    let mut upstream = forwarding
        .take_stream(resource.ports.remote)
        .context("Failed to take stream")?;

    let (mut sink, mut stream) = UdpFramed::new(server, BytesCodec::new()).split();

    stream
        .take_until(token.cancelled())
        .try_for_each_concurrent(MAX_CONCURRENT_CONNECTIONS, |(frame, _)| async move {
            let len = frame.len().to_le_bytes();
            let mut packet = Buf::chain(&len[..], frame);

            upstream.write_all_buf(&mut packet).await?;
            upstream.flush().await?;

            Ok(())
        })
        .await?;

    Ok(())
}
