#!/bin/bash
# setup_manual_test_env.sh - Sets up environment for manual testing of rsenv v2
#
# Creates test directory structure correlating with testplan-consolidation.md:
# - Section 1: env hierarchy (base.env <- dev.env <- local.env)
# - Section 2-3: project directories for init/guard/reconnect testing
# - Section 4: files for swap testing
# - Section 5: files for sops testing (optional)
#
# Usage:
#   ./setup_manual_test_env.sh [TEST_ROOT]
#   TEST_ROOT defaults to a temp directory if not specified
#
# After running:
#   cd $TEST_ROOT
#   export PATH="$RSENV_BIN:$PATH"  # if not already in PATH
#   # Follow test plan sections 1-8

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Determine test root
TEST_ROOT="${1:-$(mktemp -d)}"
# TEST_ROOT="$HOME/xxx"

info "Setting up manual test environment at: $TEST_ROOT"

# Create directory structure
mkdir -p "$TEST_ROOT"/{envs,project1/config,project2,swap_test,sops_test}

# ============================================================
# Section 1: Environment Hierarchy
# ============================================================
info "Creating env file hierarchy (Section 1)..."

cat > "$TEST_ROOT/envs/base.env" << 'EOF'
# Base environment - root of hierarchy
export APP_NAME=myapp
export LOG_LEVEL=info
export DATABASE_HOST=localhost
export DATABASE_PORT=5432
EOF

cat > "$TEST_ROOT/envs/dev.env" << 'EOF'
# rsenv: base.env
# Development environment
export DEBUG=true
export LOG_LEVEL=debug
export API_URL=http://localhost:3000
EOF

cat > "$TEST_ROOT/envs/local.env" << 'EOF'
# rsenv: dev.env
# Local overrides
export SECRET_KEY=test-secret-123
export DATABASE_HOST=127.0.0.1
export EXTRA_VAR=local-only
EOF

cat > "$TEST_ROOT/envs/standalone.env" << 'EOF'
# No parent - standalone file
export STANDALONE=true
export ISOLATED_VAR=value
EOF

success "Created env hierarchy: base.env <- dev.env <- local.env"

# ============================================================
# Section 2-4: Project directories for init/guard/swap/reconnect
# ============================================================
info "Creating project directories (Sections 2-4)..."

# project1 - for init, guard, and reconnect testing
cat > "$TEST_ROOT/project1/secrets.env" << 'EOF'
# Sensitive file for guard testing
export SECRET=password123
export API_TOKEN=sk-12345-abcde
EOF

cat > "$TEST_ROOT/project1/config/api.key" << 'EOF'
# Nested config file for guard testing
export API_KEY=sk-12345
export API_SECRET=super-secret-value
EOF

# project2 - additional project for multi-project testing
cat > "$TEST_ROOT/project2/app.env" << 'EOF'
# Second project env file
export PROJECT2_VAR=value
EOF

success "Created project1/ and project2/ directories"

# ============================================================
# Section 4: Swap testing
# ============================================================
info "Creating swap test files (Section 4)..."

cat > "$TEST_ROOT/swap_test/db_config.yml" << 'EOF'
# Database configuration
database:
  host: prod.example.com
  port: 5432
  name: production_db
EOF

cat > "$TEST_ROOT/swap_test/settings.json" << 'EOF'
{
  "environment": "production",
  "debug": false,
  "api_endpoint": "https://api.example.com"
}
EOF

success "Created swap_test/ directory with db_config.yml and settings.json"

# ============================================================
# Section 5: SOPS testing (optional)
# ============================================================
info "Creating sops test files (Section 5 - optional)..."

cat > "$TEST_ROOT/sops_test/sensitive.env" << 'EOF'
# File for SOPS encryption testing
export DB_PASSWORD=supersecret
export API_KEY=sk-live-123456
EOF

success "Created sops_test/ directory"

# ============================================================
# README with quick reference
# ============================================================
cat > "$TEST_ROOT/README.txt" << EOF
rsenv v2 Manual Test Environment
================================

Created: $(date +%Y-%m-%d)
Test Root: $TEST_ROOT

Directory Structure:
  envs/           - Section 1: Environment hierarchy tests
  project1/       - Sections 2-4: Init, guard, reconnect tests
  project2/       - Additional project for multi-project tests
  swap_test/      - Section 4: Swap in/out tests
  sops_test/      - Section 5: SOPS encryption tests (optional)

Quick Start:
  cd $TEST_ROOT

  # Section 1: Test env hierarchy
  rsenv env tree envs/
  rsenv env build envs/local.env

  # Section 2: Init project
  rsenv init project1
  rsenv info -C project1

  # Section 2.4: Reconnect (after init)
  VAULT_PATH=\$(rsenv info -C project1 | grep Vault | awk '{print \$2}')
  rm project1/.envrc
  rsenv init reconnect \$VAULT_PATH/dot.envrc -C project1

  # Section 3: Guard files
  rsenv guard add project1/secrets.env
  rsenv guard list -C project1

  # Section 4: Swap files
  cd swap_test && rsenv init
  rsenv swap init db_config.yml
  rsenv swap status

Cleanup:
  rm -rf $TEST_ROOT
  # Optional: rm -rf ~/.rsenv/vaults/project*
EOF

# ============================================================
# Summary
# ============================================================
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Test environment created successfully!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "TEST_ROOT=$TEST_ROOT"
echo ""
echo "To use:"
echo "  cd $TEST_ROOT"
echo "  cat README.txt"
echo ""
echo "To cleanup:"
echo "  rm -rf $TEST_ROOT"
