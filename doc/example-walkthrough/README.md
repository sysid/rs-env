# Example Walkthrough

This directory contains a realistic example of hierarchical environment configuration used throughout the rs-env documentation.

## Structure

```
base.env              # Shared base configuration
  └─ cloud.env        # Cloud-specific overrides
      └─ production.env  # Production-specific overrides
dev.env               # Standalone development environment
```

## Files

- **base.env**: Base configuration shared across all environments
  - DATABASE_HOST, DATABASE_PORT, LOG_LEVEL, APP_NAME

- **cloud.env**: Inherits from base.env, overrides for cloud deployments
  - Overrides: DATABASE_HOST, LOG_LEVEL
  - Adds: CLOUD_PROVIDER, REGION

- **production.env**: Inherits from cloud.env, production-specific settings
  - Overrides: DATABASE_HOST, LOG_LEVEL, REGION
  - Adds: ENVIRONMENT, API_KEY, ENABLE_MONITORING

- **dev.env**: Standalone file for local development (no parent)
  - Independent configuration for development workflow

## Usage

```bash
# Build production environment (inherits base → cloud → production)
rsenv build production.env

# Build development environment (standalone)
rsenv build dev.env

# View the hierarchy
rsenv tree .

# List all leaf environments
rsenv leaves .
```
