# kubef

A fast, intelligent Kubernetes port forwarding tool with configuration-based resource management and automatic pod selection.

## Overview

`kubef` is a command-line tool that simplifies Kubernetes port forwarding by providing:

- **Configuration-driven forwarding** - Define your forwarding rules in YAML configuration files
- **Intelligent pod selection** - Automatically discovers and forwards to healthy pods using label selectors, services, or deployments
- **Load balancing** - Distributes incoming connections across available pods
- **Resource grouping** - Organize resources into logical groups with aliases for easy management
- **Real-time pod watching** - Automatically adapts to pod changes using Kubernetes watch API
- **High performance** - Built in Rust with async/await for efficient connection handling

## Installation

### Prerequisites

- Rust 2024 edition or later
- Access to a Kubernetes cluster with `kubectl` configured

### From source

```bash
git clone https://github.com/marcocondrache/kubef && cd kubef
cargo install --path .
```

This installs `kubef` to `~/.cargo/bin/`. Make sure this directory is in your PATH.

### Manual installation

Alternatively, build and copy manually:

```bash
cargo build --release
cp target/release/kubef /usr/local/bin/
```

## Configuration

`kubef` uses YAML configuration files to define forwarding rules. By default, it looks for configuration in:

- `$KUBEF_CONFIG` (if set)
- `~/.config/kubef/config.yaml` (XDG config directory)

### Configuration Format

```yaml
# Optional: default kubeconfig context for all resources
context: <kubeconfig-context-or-alias>

# Optional: distinct loopback IPs per resource (macOS only — adds lo0 aliases)
loopback: 127.0.0.0/8

# Optional: short names for verbose kubeconfig context names
contexts:
  <alias>:
    kubeconfig: <raw-kubeconfig-context-name>
    namespace: <optional-default-namespace>

# Optional: global default for remote port resolution (default: container)
ports:
  mapping: container | service

groups:
  <group_name>:
    - alias: <resource_alias>
      namespace: <namespace>        # optional; falls back to context alias namespace, then "default"
      context: <alias-or-raw>       # optional; overrides config-level context
      policy: roundrobin | sticky   # optional; default: roundrobin
      selector:
        type: label | deployment | service
        match: <selector_value>
      ports:
        remote: <port-name-or-number> # string = named port; number = container or service port
        local: <local_port>           # optional; omit for OS-assigned
        mapping: container | service  # optional; per-resource override of global default
```

### Selector types

- **service** - Select pods via Kubernetes Service selector
- **deployment** - Select pods managed by a specific Deployment
- **label** - Select pods matching label key-value pairs

### Example configuration

```yaml
context: prod-cluster
loopback: 127.1.0.0/24

contexts:
  prod:
    kubeconfig: arn:aws:eks:eu-west-1:123456789:cluster/prod
    namespace: production
  staging:
    kubeconfig: arn:aws:eks:eu-west-1:123456789:cluster/staging
    namespace: staging

groups:
  web:
    - alias: frontend
      context: prod
      selector:
        type: service
        match: frontend-service
      ports:
        remote: http    # named port resolved from the service spec
        local: 3000

    - alias: api
      context: prod
      selector:
        type: deployment
        match: api-deployment
      ports:
        remote: 8000
        local: 8000

  development:
    - alias: pdf
      namespace: default
      selector:
        type: service
        match: pdf
      ports:
        remote: 8080
        local: 9000
```

## Context Aliases

Long kubeconfig context names (e.g. ARN strings for EKS clusters) can be aliased in the `contexts` map. Each alias carries two fields:

- `kubeconfig` — the raw kubeconfig context name to use when connecting
- `namespace` — optional default namespace for resources that use this alias

```yaml
contexts:
  prod:
    kubeconfig: arn:aws:eks:eu-west-1:123456789:cluster/prod
    namespace: production
  local:
    kubeconfig: minikube
```

Resources then reference the alias in their `context` field:

```yaml
groups:
  services:
    - alias: api
      context: prod     # expands to the full EKS ARN; namespace defaults to "production"
      selector:
        type: service
        match: api
      ports:
        remote: 8080
        local: 8080
```

Namespace resolution order for a resource using a context alias: explicit `namespace` on the resource → `namespace` from the context alias → `"default"`.

The config-level `context` field and per-resource `context` field both accept either a raw kubeconfig context name or a context alias.

## Named Ports and Port Mapping

`ports.remote` accepts either a number or a string (named port):

- **String** (e.g. `remote: http`) — kubef resolves the name against the service spec (for `selector.type: service`) or the full pod spec (for other selectors). An error is reported at startup if the named port is not found.
- **Number** — interpreted according to `ports.mapping`:
  - `mapping: container` (default) — the number is used directly as the container port in the pod-level port-forward
  - `mapping: service` — the number is looked up in the Service's `spec.ports[].port` to find the corresponding `targetPort`

`mapping` can be set globally under the top-level `ports` key, or overridden per resource:

```yaml
ports:
  mapping: service   # global default: treat numeric remote ports as service ports

groups:
  backend:
    - alias: auth
      selector:
        type: service
        match: auth-svc
      ports:
        remote: 80          # service port 80 → resolved to container targetPort
        local: 8080

    - alias: metrics
      selector:
        type: service
        match: metrics-svc
      ports:
        remote: 9090
        mapping: container  # override: treat 9090 as a direct container port
        local: 9090
```

## Usage

### Basic usage

Forward to a specific resource by alias:
```bash
kubef pdf
```

Forward to a resource using the `forward` subcommand:
```bash
kubef forward --target pdf
```

Forward to all resources in a group:
```bash
kubef web
```

If the alias is not found, kubef suggests close matches and exits without prompting — re-run with the corrected name:

```
error: unknown target "frotend"
  Did you mean: frontend, frontend-v2?
```

### How It Works

1. **Configuration loading** - `kubef` loads your configuration file and parses the resource definitions
2. **Resource resolution** - Based on your target (alias or group), it identifies which resources to forward
3. **Pod discovery** - For each resource, it uses the configured selector to find matching pods in the cluster
4. **Port binding** - Creates local TCP listeners on the specified local ports
5. **Connection forwarding** - When a connection arrives, it selects an available pod and establishes a port-forward tunnel
6. **Real-time updates** - Continuously watches for pod changes and updates the available target pool

### Advanced Features

- **Load balancing** - Automatically distributes connections across healthy pods
- **Fault tolerance** - Handles pod restarts and failures gracefully
- **Signal handling** - Clean shutdown on Ctrl+C
- **Structured Logging** - Detailed logging with configurable levels via `KUBEF_LOG` environment variable

## Examples

### Simple service forwarding

```yaml
groups:
  services:
    - alias: webapp
      selector:
        type: service
        match: webapp-service
      ports:
        remote: 80
        local: 8080
```

```bash
kubef webapp
# Access your webapp at http://localhost:8080
```

### Multi-Environment setup

```yaml
groups:
  staging:
    - alias: api
      namespace: staging
      selector:
        type: deployment
        match: api
      ports:
        remote: 8000
        local: 8001

  production:
    - alias: api
      namespace: production
      selector:
        type: deployment
        match: api
      ports:
        remote: 8000
        local: 8002
```

```bash
# Forward staging API
kubef staging

# Or target specific environment
kubef api  # Will forward all 'api' aliases
```

### Label-based selection

```yaml
groups:
  monitoring:
    - alias: prometheus
      selector:
        type: label
        match:
          - ["app", "prometheus"]
          - ["component", "server"]
      ports:
        remote: 9090
        local: 9090
```

## Shell Completions

`kubef` completes subcommand names, flags, and config-driven aliases and group names at tab-press time. Completions are driven by the binary itself — aliases are read from the active config file on every tab-press, so newly added aliases appear immediately.

### Installation

Add the appropriate line to your shell init file and reload.

**Bash** (`~/.bashrc`):
```bash
source <(COMPLETE=bash kubef)
```

**Zsh** (`~/.zshrc`):
```zsh
source <(COMPLETE=zsh kubef)
```

**Fish** (`~/.config/fish/config.fish`):
```fish
COMPLETE=fish kubef | source
```

**Elvish** (`~/.elvish/rc.elv`):
```elvish
eval (E:COMPLETE=elvish kubef | slurp)
```

**PowerShell** (`$PROFILE`):
```powershell
$env:COMPLETE = "powershell"; kubef | Out-String | Invoke-Expression; Remove-Item Env:\COMPLETE
```

## Environment Variables

- `KUBEF_CONFIG` - Custom path to configuration file
- `KUBEF_LOG` - Set logging level (e.g., `KUBEF_LOG=debug kubef webapp`)

## Development

### Prerequisites

- Rust 2024 edition
- Kubernetes cluster for testing

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Code Structure

- `src/main.rs` - Application entry point
- `src/cli/` - Command-line interface and argument parsing
- `src/cnf/` - Configuration management and parsing
- `src/fwd/` - Core port forwarding logic and pod watching
- `src/fwd/watcher.rs` - Kubernetes pod watcher implementation

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Troubleshooting

### Common Issues

1. **"No resources found"** - Check that your configuration file exists and contains the specified alias or group. If the name is close but not exact, kubef will suggest alternatives — check the error output for "Did you mean" hints
2. **Connection refused** - Ensure the target pods are running and the remote port is correct
3. **Permission denied** - Verify your kubectl configuration and cluster access

### Debugging

Enable debug logging to see detailed information:

```bash
KUBEF_LOG=debug kubef webapp
```

This will show configuration loading, pod discovery, and connection forwarding details.
