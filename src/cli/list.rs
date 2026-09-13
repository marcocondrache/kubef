use std::fmt::Write as _;

use anyhow::Result;

use crate::config::{
    self,
    schema::{Config, PortSpec, Resource, ResourceSelector},
};

pub async fn init() -> Result<()> {
    let path = config::config_path()?;
    let config = config::extract().await?;
    print!("{}", render(config, &path));
    Ok(())
}

pub fn render(config: &Config, path: &std::path::Path) -> String {
    let mut out = format!("# {}\n", path.display());

    if config.groups.is_empty() {
        out.push_str("No resources configured. Edit this file or run `kubef init --force`.\n");
        return out;
    }

    let mut groups: Vec<(&String, &Vec<Resource>)> = config.groups.iter().collect();
    groups.sort_by(|a, b| a.0.cmp(b.0));

    for (name, resources) in groups {
        out.push('\n');
        out.push_str(name);
        out.push('\n');

        if resources.is_empty() {
            out.push_str("  (empty group)\n");
            continue;
        }

        for resource in resources {
            let context = resource
                .context
                .as_deref()
                .map(|ctx| format!("  {ctx}"))
                .unwrap_or_default();
            writeln!(
                out,
                "  {:<16} {:<28} {} → {}{context}",
                resource.alias,
                selector_label(&resource.selector),
                local_port(resource),
                remote_port(&resource.ports.remote),
            )
            .expect("write to String");
        }
    }

    out
}

fn selector_label(selector: &ResourceSelector) -> String {
    match selector {
        ResourceSelector::Service(name) => format!("service/{name}"),
        ResourceSelector::Deployment(name) => format!("deployment/{name}"),
        ResourceSelector::Label(labels) => {
            let pairs = labels
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("label/{pairs}")
        }
    }
}

fn remote_port(remote: &PortSpec) -> String {
    match remote {
        PortSpec::Named(name) => name.clone(),
        PortSpec::Number(port) => port.to_string(),
    }
}

fn local_port(resource: &Resource) -> String {
    resource
        .ports
        .local
        .map_or_else(|| "ephemeral".to_string(), |port| port.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{Ports, ResourceSelector};
    use std::collections::HashMap;
    use std::path::Path;

    fn resource(
        alias: &str,
        selector: ResourceSelector,
        remote: PortSpec,
        local: Option<u16>,
    ) -> Resource {
        Resource {
            alias: alias.to_string(),
            namespace: None,
            context: Some("prod".to_string()),
            policy: None,
            selector,
            ports: Ports {
                remote,
                local,
                mapping: None,
            },
        }
    }

    #[test]
    fn render_empty_points_at_init() {
        let text = render(
            &Config {
                context: None,
                groups: HashMap::new(),
                loopback: None,
                contexts: HashMap::new(),
                ports: None,
            },
            Path::new("/tmp/config.yaml"),
        );
        assert!(text.contains("/tmp/config.yaml"));
        assert!(text.contains("kubef init"));
    }

    #[test]
    fn render_groups_sorted_with_ports() {
        let mut groups = HashMap::new();
        groups.insert(
            "web".to_string(),
            vec![resource(
                "frontend",
                ResourceSelector::Service("frontend-svc".into()),
                PortSpec::Named("http".into()),
                Some(3000),
            )],
        );
        groups.insert(
            "dev".to_string(),
            vec![resource(
                "pdf",
                ResourceSelector::Deployment("pdf".into()),
                PortSpec::Number(8080),
                None,
            )],
        );

        let text = render(
            &Config {
                context: None,
                groups,
                loopback: None,
                contexts: HashMap::new(),
                ports: None,
            },
            Path::new("/cfg.yaml"),
        );

        let dev = text.find("\ndev\n").unwrap();
        let web = text.find("\nweb\n").unwrap();
        assert!(dev < web);
        assert!(text.contains("frontend"));
        assert!(text.contains("service/frontend-svc"));
        assert!(text.contains("3000 → http"));
        assert!(text.contains("ephemeral → 8080"));
        assert!(text.contains("prod"));
    }
}
