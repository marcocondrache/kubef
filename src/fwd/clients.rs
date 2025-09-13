use anyhow::{Context, Result};
use dashmap::{DashMap, Entry};
use kube::{Client, Config, config::KubeConfigOptions};
use tokio::sync::OnceCell;

#[derive(Default)]
pub struct ClientPool<'ctx> {
    default: OnceCell<Client>,
    clients: DashMap<&'ctx str, Client>,
}

impl<'ctx> ClientPool<'ctx> {
    pub async fn get_default(&self) -> Result<Client> {
        self.default
            .get_or_try_init(Client::try_default)
            .await
            .context("Failed to get default client")
            .cloned()
    }

    pub async fn get_or_insert(&self, context: &'ctx str) -> Result<Client> {
        match self.clients.entry(context) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let config = Config::from_kubeconfig(&KubeConfigOptions {
                    context: Some(context.to_owned()),
                    cluster: None,
                    user: None,
                })
                .await?;

                Ok(entry.insert(Client::try_from(config)?).clone())
            }
        }
    }
}
