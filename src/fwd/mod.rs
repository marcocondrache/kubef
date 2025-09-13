use std::sync::Arc;

use crate::{
    cnf::schema::{Resource, ResourceSelector},
    fwd::{
        clients::ClientPool,
        sockets::{LoopbackToken, SocketPool},
    },
};
use anyhow::{Context, Result};
use either::Either;
use ipnet::IpNet;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Pod, Service},
};
use kube::{
    Api, Client, ResourceExt,
    core::{Expression, Selector},
};
use tokio::net::{TcpSocket, TcpStream};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{Level, debug, info, instrument};

mod clients;
mod sockets;
mod watcher;

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
        &mut self,
        socket: TcpSocket,
        resource: &'static Resource,
        ltoken: Option<LoopbackToken>,
    ) -> Result<impl Future<Output = Result<()>> + 'fut> {
        let token = self.token.child_token();
        let tracker = self.tracker.clone();

        let policy = resource.policy.unwrap_or_default();
        let context = resource.context.as_deref().or(self.context);

        let client = match context {
            Some(context) => self.pool.get_or_insert(context).await?,
            _ => self.pool.get_default().await?,
        };

        let server = socket.listen(1024)?;

        let api = Api::<Pod>::namespaced(client.clone(), &resource.namespace);

        info!(
            "Listening TCP on {} forwarded to {}",
            server.local_addr()?,
            resource.alias
        );

        // TODO: How do we capture the error?
        let future = async move {
            let selector = select(client, resource).await?;
            let mut watcher = watcher::PodWatcher::new(api.clone(), &selector, policy).await?;

            let api_ptr = Arc::new(api);

            loop {
                tokio::select! {
                    biased;
                    () = token.cancelled() => break,
                    Ok((connection, addr)) = server.accept() => {
                        let api = api_ptr.clone();
                        let token = token.child_token();

                        let pod = match watcher.get() {
                            Some(pod) => pod,
                            None => token.run_until_cancelled(watcher.next()).await.transpose()?.context("Failed to get next pod")?,
                        };

                        let pod_name = pod.name_any();
                        let pod_port = resource.ports.remote;

                        info!(
                            "Forwarding connection from {} to {}",
                            addr,
                            pod_name
                        );

                        tracker.spawn(forward(api, pod_port, pod_name, connection, token));
                    }
                    else => break,
                }
            }

            drop(ltoken);

            Ok(())
        };

        Ok(future)
    }

    pub async fn forward(&mut self, resource: &'static Resource) -> Result<()> {
        let (socket, ltoken) = self.sockets.get_loopback(resource.ports.local).await?;
        let future = self.bind(socket, resource, ltoken).await?;

        self.tracker.spawn(future);

        Ok(())
    }

    pub async fn forward_all(&mut self, resources: &'static [Resource]) -> Result<()> {
        for resource in resources {
            self.forward(resource).await?;
        }

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.token.cancel();
        self.tracker.close();
        self.tracker.wait().await;

        Ok(())
    }
}

#[instrument(err(level = Level::WARN), skip(api, connection, token), fields(pod_name = %pod_name.as_ref()))]
pub async fn forward(
    api: Arc<Api<Pod>>,
    pod_port: u16,
    pod_name: impl AsRef<str>,
    mut connection: TcpStream,
    token: CancellationToken,
) -> Result<()> {
    // Optimization
    connection.set_nodelay(true)?;
    connection.set_linger(None)?;

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

    tokio::select! {
        biased;
        () = token.cancelled() => {},
        Some(e) = closer => {
            forwarding.abort();

            anyhow::bail!(e);
        }
        Err(e) = tokio::io::copy_bidirectional(&mut connection, &mut upstream) => {
            forwarding.abort();

            anyhow::bail!(e);
        }
    };

    debug!("Going to gracefully drop upstream connection");

    drop(upstream);

    forwarding
        .join()
        .await
        .context("Failed to conclude forward")
}

pub async fn select(client: Client, resource: &Resource) -> Result<Selector> {
    match &resource.selector {
        ResourceSelector::Label(labels) => {
            let result = labels
                .iter()
                .map(|(k, v)| Expression::In(k.to_owned(), [v.to_owned()].into()))
                .collect::<Selector>();

            Ok(result)
        }
        ResourceSelector::Deployment(name) => {
            let api: Api<Deployment> = Api::namespaced(client, &resource.namespace);
            let deployment = api.get(name).await?;
            let selector = deployment.spec.context("Deployment has no spec")?.selector;

            let result = selector
                .match_labels
                .context("Deployment has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Selector>();

            Ok(result)
        }
        ResourceSelector::Hostname(name) => {
            let service_name = name.split('.').next().unwrap_or(name);

            let api: Api<Service> = Api::namespaced(client, &resource.namespace);
            let service = api.get(service_name).await?;
            let selector = service.spec.context("Service has no spec")?.selector;

            let result = selector
                .context("Service has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Selector>();

            Ok(result)
        }
        ResourceSelector::Service(name) => {
            let api: Api<Service> = Api::namespaced(client, &resource.namespace);
            let service = api.get(name).await?;
            let selector = service.spec.context("Service has no spec")?.selector;

            let result = selector
                .context("Service has no selector")?
                .into_iter()
                .map(|(k, v)| Expression::In(k, [v].into()))
                .collect::<Selector>();

            Ok(result)
        }
    }
}
