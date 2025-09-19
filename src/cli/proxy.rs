use crate::fwd::clients::ClientPool;
use anyhow::Result;
use clap::Args;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, api::PostParams};
use leon::Template;

static POD_TEMPLATE: Template<'_> = leon::template!(
    r#"
  apiVersion: v1
  kind: Pod
  metadata:
    name: kubef-{pod_id}
    labels:
      kubef.io/id: {pod_id}
      kubef.io/proxy: "true"
  spec:
    containers:
      - name: socat
        image: {pod_image}
        command: 
          - socat
          - {protocol}-LISTEN:{pod_port},fork
          - {protocol}:{remote_ip}:{remote_port}
"#
);

#[derive(Args)]
pub struct ProxyCommandArguments {
    #[arg(
        short,
        long,
        default_value = "alpine/socat:latest",
        help = "Socat image to use"
    )]
    pub image: String,

    #[arg(short, long, help = "Namespace to use")]
    pub namespace: Option<String>,

    #[arg(short, long, help = "Local port to listen on")]
    pub pod_port: u16,

    #[arg(short, long, help = "Remote port to forward to")]
    pub remote_port: u16,

    #[arg(short, long, help = "Remote IP to forward to")]
    pub remote_ip: String,

    #[arg(short, long, default_value = "TCP", help = "Protocol to use")]
    pub protocol: String,

    #[arg(short, long, help = "The kubeconfig context to use")]
    pub context: Option<String>,
}

pub async fn init(
    ProxyCommandArguments {
        context,
        image,
        pod_port,
        remote_port,
        remote_ip,
        protocol,
        namespace,
        ..
    }: ProxyCommandArguments,
) -> Result<()> {
    let pool = ClientPool::default();
    let client = match context {
        Some(context) => pool.get_or_insert(&context).await?,
        None => pool.get_default().await?,
    };

    let namespace = namespace.as_deref().unwrap_or(client.default_namespace());
    let api = Api::<Pod>::namespaced(client.clone(), namespace);

    let parameters = [
        ("pod_id", "1"),
        ("pod_image", &image),
        ("pod_port", &pod_port.to_string()),
        ("remote_port", &remote_port.to_string()),
        ("remote_ip", &remote_ip),
        ("protocol", &protocol),
    ];

    let manifest = POD_TEMPLATE.render(&parameters)?;
    let pod = serde_json::from_str::<Pod>(&manifest)?;

    api.create(&PostParams::default(), &pod).await?;

    Ok(())
}
