use anyhow::Result;
use kube::{Client, Config, config::KubeConfigOptions};
use std::collections::{HashMap, hash_map::Entry};

pub struct ClientPool {
    default: Client,
    clients: HashMap<String, Client>,
}

impl ClientPool {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            default: Client::try_default().await?,
            clients: HashMap::new(),
        })
    }

    pub fn default(&self) -> Client {
        self.default.clone()
    }

    pub async fn get_or_insert(&mut self, context: &str) -> Result<Client> {
        match self.clients.entry(context.to_string()) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => {
                let config = Config::from_kubeconfig(&KubeConfigOptions {
                    context: Some(context.to_string()),
                    cluster: None,
                    user: None,
                })
                .await?;

                Ok(entry.insert(Client::try_from(config)?).clone())
            }
        }
    }
}
