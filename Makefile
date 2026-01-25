.DEFAULT_GOAL := help

# Configuration
PREFIX ?= /usr/local
BINPREFIX ?= $(PREFIX)/bin
VERSION = $(shell cat VERSION)
SHELL = bash
.ONESHELL:

# Paths
app_root := $(CURDIR)
pkg_src = $(app_root)/rsenv
BINARY = rsenv

################################################################################
# Development                                                                  #
################################################################################

.PHONY: test
test:  ## Run all tests (single-threaded, required)
	cd $(pkg_src) && cargo test -- --test-threads=1

.PHONY: test-verbose
test-verbose:  ## Run tests with debug logging
	cd $(pkg_src) && RUST_LOG=DEBUG cargo test -- --test-threads=1 --nocapture

.PHONY: watch
watch:  ## Run tests on file changes (requires cargo-watch)
	cd $(pkg_src) && cargo watch -x 'test -- --test-threads=1'

.PHONY: format
format:  ## Format code
	cd $(pkg_src) && cargo fmt

.PHONY: lint
lint:  ## Lint and auto-fix
	cd $(pkg_src) && cargo clippy --fix --allow-dirty --allow-staged
	cd $(pkg_src) && cargo fix --lib -p rsenv --tests --allow-dirty --allow-staged

.PHONY: check
check:  ## Quick compile check (no codegen)
	cd $(pkg_src) && cargo check

.PHONY: doc
doc:  ## Generate and open documentation
	cd $(pkg_src) && cargo doc --open

################################################################################
# Building                                                                     #
################################################################################

.PHONY: build
build:  ## Build release version
	cd $(pkg_src) && cargo build --release

.PHONY: build-debug
build-debug:  ## Build debug version
	cd $(pkg_src) && cargo build

.PHONY: all
all: clean build install  ## Clean, build release, and install

.PHONY: all-debug
all-debug: clean build-debug install-debug  ## Clean, build debug, and install

################################################################################
# Installation                                                                 #
################################################################################

# macOS requires re-signing after copy (adhoc signature becomes invalid)
define install_binary
	@echo "-M- Installing $(BINARY) v$(VERSION) from $(1)"
	cp -f $(1) ~/bin/$(BINARY)$(VERSION)
	codesign --force --sign - ~/bin/$(BINARY)$(VERSION)
	ln -sf ~/bin/$(BINARY)$(VERSION) ~/bin/$(BINARY)
	~/bin/$(BINARY) completion bash > ~/.bash_completions/$(BINARY) 2>/dev/null || true
endef

.PHONY: install
install: uninstall  ## Install release binary to ~/bin
	$(call install_binary,$(pkg_src)/target/release/$(BINARY))

.PHONY: install-debug
install-debug: uninstall  ## Install debug binary to ~/bin
	$(call install_binary,$(pkg_src)/target/debug/$(BINARY))

.PHONY: uninstall
uninstall:  ## Remove installed binary
	@rm -f ~/bin/$(BINARY) ~/bin/$(BINARY)[0-9]* ~/.bash_completions/$(BINARY)

################################################################################
# Release                                                                      #
################################################################################

.PHONY: check-github-token
check-github-token:
	@test -n "$$GITHUB_TOKEN" || { echo "Error: GITHUB_TOKEN not set"; exit 1; }
	@echo "GITHUB_TOKEN is set"

.PHONY: bump-major
bump-major: check-github-token  ## Bump major version, tag, push, create release
	bump-my-version bump --commit --tag major
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: bump-minor
bump-minor: check-github-token  ## Bump minor version, tag, push, create release
	bump-my-version bump --commit --tag minor
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: bump-patch
bump-patch: check-github-token  ## Bump patch version, tag, push, create release
	bump-my-version bump --commit --tag patch
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: bump-prenum
bump-prenum: check-github-token  ## Bump pre-release number (alpha.1 → alpha.2)
	bump-my-version bump --commit --tag prenum
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: bump-pre
bump-pre: check-github-token  ## Bump pre-release stage (alpha → beta → rc)
	bump-my-version bump --commit --tag pre
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: release
release: check-github-token  ## Release current version (2.0.0-alpha.1 → 2.0.0)
	@RELEASE_VERSION=$$(cat VERSION | sed 's/-.*//') && \
	bump-my-version bump --commit --tag --new-version $$RELEASE_VERSION
	git push && git push --tags
	@$(MAKE) create-release

.PHONY: create-release
create-release: check-github-token  ## Create GitHub release via gh CLI
	@command -v gh >/dev/null || { echo "gh CLI not installed"; exit 1; }
	@if echo "$(VERSION)" | grep -qE '-(alpha|beta|rc)'; then \
		gh release create "v$(VERSION)" --generate-notes --prerelease; \
	else \
		gh release create "v$(VERSION)" --generate-notes --latest; \
	fi

.PHONY: upload
upload:  ## Publish to crates.io
	@test -n "$$CARGO_REGISTRY_TOKEN" || { echo "Error: CARGO_REGISTRY_TOKEN not set"; exit 1; }
	cd $(pkg_src) && cargo publish

################################################################################
# Clean                                                                        #
################################################################################

.PHONY: clean
clean:  ## Clean build artifacts
	cd $(pkg_src) && cargo clean

################################################################################
# Help                                                                         #
################################################################################

define PRINT_HELP_PYSCRIPT
import re, sys

for line in sys.stdin:
	match = re.match(r'^([%a-zA-Z0-9_-]+):.*?## (.*)$$', line)
	if match:
		target, help = match.groups()
		if target != "dummy":
			print("\033[36m%-20s\033[0m %s" % (target, help))
endef
export PRINT_HELP_PYSCRIPT

.PHONY: help
help:
	@python -c "$$PRINT_HELP_PYSCRIPT" < $(MAKEFILE_LIST)


.PHONY: debug
debug:  ## Show Makefile variables
	@echo "VERSION:  $(VERSION)"
	@echo "app_root: $(app_root)"
	@echo "pkg_src:  $(pkg_src)"
	@echo "BINARY:   $(BINARY)"
