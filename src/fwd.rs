use std::sync::Arc;

use crate::{
    cnf::{self, schema::Resource},
    fwd::{
        clients::ClientPool,
        sockets::{LoopbackToken, SocketPool},
    },
};
use anyhow::{Context, Result};
use either::Either;
use futures::future;
use ipnet::IpNet;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, ResourceExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{Level, debug, info, instrument, warn};

pub mod clients;
pub mod proxy;
pub mod sockets;
pub mod watcher;

pub type Target<'a> = Either<&'a Resource, &'a [Resource]>;

#[derive(Default)]
pub struct Forwarder<'ctx> {
    pool: ClientPool<'ctx>,
    sockets: SocketPool,
    tracker: TaskTracker,
    token: CancellationToken,
    context: Option<&'ctx str>,
}

impl<'ctx> Forwarder<'ctx> {
    pub fn with_context(mut self, context: impl Into<Option<&'ctx str>>) -> Self {
        self.context = context.into();
        self
    }

    pub fn with_loopback(mut self, loopback: impl Into<Option<IpNet>>) -> Self {
        self.sockets = self.sockets.with_loopback(loopback.into());
        self
    }

    #[instrument(err, skip(self, socket, resource, ltoken), fields(resource = %resource.alias))]
    pub async fn bind<'fut>(
        &self,
        socket: TcpSocket,
        resource: &'static Resource,
        ltoken: Option<LoopbackToken>,
    ) -> Result<impl Future<Output = Result<()>> + 'fut> {
        let token = self.token.child_token();
        let tracker = self.tracker.clone();

        let policy = resource.policy.unwrap_or_default();
        let context = resource.context.as_deref().or(self.context);

        let config = cnf::extract().await?;

        let (kubeconfig_context, alias_namespace) = match context {
            Some(ctx) => match config.contexts.get(ctx) {
                Some(alias) => (Some(alias.kubeconfig.as_str()), alias.namespace.as_deref()),
                None => (Some(ctx), None),
            },
            None => (None, None),
        };

        let client = match kubeconfig_context {
            Some(ctx) => self.pool.get_or_insert(ctx).await?,
            None => self.pool.get_default().await?,
        };

        let namespace = resource
            .namespace
            .as_deref()
            .or(alias_namespace)
            .unwrap_or("default");

        let server = socket.listen(1024)?;

        let api_ptr = Arc::new(Api::<Pod>::namespaced(client.clone(), namespace));
        let meta_api =
            Api::<kube::api::PartialObjectMeta<Pod>>::namespaced(client.clone(), namespace);

        info!(
            "Listening TCP on {} forwarded to {}",
            server.local_addr()?,
            resource.alias
        );

        let pod_port = watcher::resolve_port(&client, resource, config).await?;

        // TODO: How do we capture the error?
        let future = async move {
            let selector = watcher::select(&client, resource, config).await?;
            let mut watcher = watcher::Watcher::new(meta_api, &selector, policy).await?;

            loop {
                tokio::select! {
                    biased;
                    () = token.cancelled() => break,
                    // Wait for next pod before accepting new connections
                    _ = watcher.next(), if watcher.is_empty() => {},
                    Ok((connection, addr)) = server.accept() => {
                        let api = api_ptr.clone();

                        let Some(pod) = watcher.get() else { continue };

                        let pod_name = pod.name_any();

                        info!(
                            "Forwarding connection from {} to {}",
                            addr,
                            pod_name
                        );

                        tracker.spawn(Forwarder::upstream(api, pod_port, pod_name, connection, token.child_token()));
                    }
                }
            }

            drop(ltoken);

            Ok(())
        };

        Ok(future)
    }

    pub async fn forward(&self, resource: &'static Resource) -> Result<()> {
        let (socket, ltoken) = self.sockets.get_loopback(resource.ports.local).await?;
        let future = self.bind(socket, resource, ltoken).await?;

        self.tracker.spawn(future);

        Ok(())
    }

    pub async fn forward_all(&self, resources: &'static [Resource]) -> Result<()> {
        future::join_all(resources.iter().map(|resource| self.forward(resource)))
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.token.cancel();
        self.tracker.close();
        self.tracker.wait().await;

        Ok(())
    }
}

impl Forwarder<'_> {
    #[instrument(err(level = Level::WARN), skip(api, connection, token), fields(pod_name = %pod_name.as_ref()))]
    pub async fn upstream(
        api: Arc<Api<Pod>>,
        pod_port: u16,
        pod_name: impl AsRef<str>,
        mut connection: TcpStream,
        token: CancellationToken,
    ) -> Result<()> {
        // Optimization
        connection.set_nodelay(true)?;

        debug!("Opening upstream connection to {}", pod_name.as_ref());

        let ports = [pod_port];
        let mut forwarding = api.portforward(pod_name.as_ref(), &ports).await?;
        let mut upstream = forwarding
            .take_stream(pod_port)
            .context("Failed to take stream")?;

        let closer = forwarding
            .take_error(pod_port)
            .context("Failed to take error stream")?;

        debug!("Upstream connection opened");

        let cancelled = tokio::select! {
            biased;
            () = token.cancelled() => true,
            Some(e) = closer => {
                forwarding.abort();

                anyhow::bail!(e);
            }
            result = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {
                if let Err(e) = result {
                    anyhow::bail!(e);
                }

                false
            }
        };

        debug!("Going to gracefully drop upstream connection");

        drop(upstream);
        forwarding.abort();

        if cancelled && let Err(e) = forwarding.join().await {
            warn!("Forward concluded with error on shutdown: {e}");
        }

        Ok(())
    }
}
