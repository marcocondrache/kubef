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
git clone <repository-url>
cd kubef
cargo build --release
```

The binary will be available at `target/release/kubef`.

## Configuration

`kubef` uses YAML configuration files to define forwarding rules. By default, it looks for configuration in:

- `$KUBEF_CONFIG` (if set)
- `~/.config/kubef/config.json` (XDG config directory)

### Configuration Format

```yaml
groups:
  <group_name>:
    - alias: <resource_alias>
      namespace: <namespace>  # optional, defaults to "default"
      selector:
        type: <selector_type>
        match: <selector_value>
      ports:
        remote: <pod_port>
        local: <local_port>
```

### Selector types

- **service** - Select pods via Kubernetes service selector
- **deployment** - Select pods managed by a specific deployment
- **label** - Select pods using label key-value pairs

### Example configuration

```yaml
groups:
  web:
    - alias: frontend
      namespace: production
      selector:
        type: service
        match: frontend-service
      ports:
        remote: 8080
        local: 3000
        
    - alias: api
      namespace: production
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

### How It Works

1. **Configuration oading** - `kubef` loads your configuration file and parses the resource definitions
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

## Environment Variables

- `KUBEF_CONFIG_PATH` - Custom path to configuration file
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

1. **"No resources found"** - Check that your configuration file exists and contains the specified alias or group
2. **Connection refused** - Ensure the target pods are running and the remote port is correct
3. **Permission denied** - Verify your kubectl configuration and cluster access

### Debugging

Enable debug logging to see detailed information:

```bash
KUBEF_LOG=debug kubef webapp
```

This will show configuration loading, pod discovery, and connection forwarding details.
