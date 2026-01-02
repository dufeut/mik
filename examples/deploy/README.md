# Deployment Examples

Example configuration files for deploying mik in production.

## Contents

- `systemd/` - systemd service and socket activation files
- `nginx/` - nginx reverse proxy with rate limiting
- `prometheus/` - Prometheus alert rules
- `grafana/` - Grafana dashboard for mik metrics

## Quick Start

1. Copy systemd files to `/etc/systemd/system/`
2. Configure nginx as reverse proxy
3. Import Grafana dashboard
4. Configure Prometheus alerts

See the [Production Deployment Guide](../../docs) for complete instructions.
