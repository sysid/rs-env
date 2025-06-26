.DEFAULT_GOAL := help
#MAKEFLAGS += --no-print-directory

# You can set these variables from the command line, and also from the environment for the first two.
PREFIX ?= /usr/local
BINPREFIX ?= "$(PREFIX)/bin"

VERSION       = $(shell cat VERSION)

SHELL	= bash
.ONESHELL:

app_root := $(if $(PROJ_DIR),$(PROJ_DIR),$(CURDIR))
pkg_src =  $(app_root)/rsenv
tests_src = $(app_root)/rsenv/tests
BINARY = rsenv

# Makefile directory
CODE_DIR := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

# define files
MANS = $(wildcard ./*.md)
MAN_HTML = $(MANS:.md=.html)
MAN_PAGES = $(MANS:.md=.1)
# avoid circular targets
MAN_BINS = $(filter-out ./tw-extras.md, $(MANS))

################################################################################
# Admin \
ADMIN::  ## ##################################################################
.PHONY: init-env
init-env:  ## init-env
	@rm -fr ~/xxx/*
	@mkdir -p ~/xxx
	@cp -r $(tests_src)/resources/environments/complex/dot.envrc ~/xxx/.envrc
	cat ~/xxx/.envrc

.PHONY: show-env
show-env:  ## show-env
	@tree -a ~/xxx

.PHONY: test
test:  ## test
	RUST_LOG=DEBUG pushd $(pkg_src) && cargo test -- --test-threads=1  # --nocapture
	#RUST_LOG=DEBUG pushd $(pkg_src) && cargo test

.PHONY: run-edit-leaf
run-edit-leaf:  ## run-edit-leaf: expect to open entire branch
	pushd $(pkg_src) && cargo run -- edit-leaf tests/resources/environments/tree2/confguard/subdir/level32.env

.PHONY: run-leaves
run-leaves:  ## run-leaves: expect level21, level32, level13, level11
	pushd $(pkg_src) && cargo run -- leaves tests/resources/environments/tree2/confguard

.PHONY: run-edit
run-edit:  ## run-edit: expect fzf selection for open
	pushd $(pkg_src) && cargo run -- edit ./tests/resources/environments/complex

.PHONY: run-build
run-build:  ## run-build: expect fully compiled env vars list
	pushd $(pkg_src) && time cargo run -- -d -d build ./tests/resources/environments/complex/level4.env

.PHONY: run-select-leaf
run-select-leaf:  ## run-select-leaf: expect updated .envrc (idempotent)
	rsenv/target/debug/rsenv select-leaf $(SOPS_PATH)/environments/local.env
	cat .envrc

.PHONY: run-select
run-select:  ## run-select: select sops env and update .envrc
	rsenv/target/debug/rsenv select $(SOPS_PATH)
	cat .envrc

.PHONY: run-files
run-files:  ## run-files: create branch
	pushd $(pkg_src) && time cargo run -- -d -d files ./tests/resources/environments/complex/level4.env

### Expected .enrc entry:
# #------------------------------- rsenv start --------------------------------
# export PIPENV_VENV_IN_PROJECT=1  # creates .venv
# export PYTHONPATH=$PROJ_DIR
# export RUN_ENV=local
# export SOPS_PATH=$HOME/dev/s/private/sec-sops/confguard/rs-sops-20ae57f0
# export TERRAFORM_PROMPT=0
# export VAR_1=var_1
# export VAR_2=var_2
# export VAR_3=var_31
# export VAR_4=var_42
# export VAR_5=var_53
# export VAR_6=var_64
# export VAR_7=var_74
# export senv="source $PROJ_DIR/scripts/env.sh"
# #-------------------------------- rsenv end ---------------------------------
.PHONY: run-envrc
run-envrc: init-env  ## run-envrc: expect above entry in .envrc
	pushd $(pkg_src) && time cargo run -- -d -d envrc ./tests/resources/environments/complex/level4.env ~/xxx/.envrc
	#pushd $(pkg_src) && time cargo run -- envrc ./tests/resources/environments/complex/level4.env ~/xxx/.envrc
	cat ~/xxx/.envrc

.PHONY: test-fzf-edit
test-fzf-edit:  ## test-fzf-edit
	pushd $(pkg_src) && cargo test --package rsenv --test test_edit test_select_file_with_suffix -- --exact --nocapture --ignored

.PHONY: test-edit
test-edit:  ## test-edit
	pushd $(pkg_src) && cargo test --package rsenv --test test_edit test_open_files_in_editor -- --exact --nocapture --ignored

.PHONY: test-vimscript
test-vimscript:  ## test-vimscript
	pushd $(pkg_src) && cargo test --package rsenv --test test_edit test_create_vimscript -- --exact --nocapture --ignored

.PHONY: test-env-vars
test-env-vars:  ## test-env-vars: test environment variable resolution in rsenv comments
	@echo "=== Testing Environment Variable Resolution ==="
	@echo "Setting up RSENV_TEST_ROOT variable..."
	@export RSENV_TEST_ROOT=$(pkg_src)/tests/resources/environments/env_vars && \
	echo "RSENV_TEST_ROOT=$$RSENV_TEST_ROOT" && \
	echo "" && \
	echo "=== Testing build command with \$$VAR syntax ===" && \
	pushd $(pkg_src) && cargo run -- build tests/resources/environments/env_vars/development.env && \
	echo "" && \
	echo "=== Testing build command with \$${VAR} syntax ===" && \
	cargo run -- build tests/resources/environments/env_vars/production.env && \
	echo "" && \
	echo "=== Testing tree command ===" && \
	cargo run -- tree tests/resources/environments/env_vars && \
	echo "" && \
	echo "=== Testing branches command ===" && \
	cargo run -- branches tests/resources/environments/env_vars && \
	echo "" && \
	echo "=== Testing files command ===" && \
	cargo run -- files tests/resources/environments/env_vars/staging.env && \
	echo "" && \
	echo "✓ All tests completed successfully!"

################################################################################
# Building, Deploying \
BUILDING:  ## ##################################################################

.PHONY: doc
doc:  ## doc
	@rustup doc --std
	pushd $(pkg_src) && cargo doc --open

.PHONY: all
all: clean build install  ## all
	:

.PHONY: upload
upload:  ## upload
	@if [ -z "$$CARGO_REGISTRY_TOKEN" ]; then \
		echo "Error: CARGO_REGISTRY_TOKEN is not set"; \
		exit 1; \
	fi
	@echo "CARGO_REGISTRY_TOKEN is set"
	pushd $(pkg_src) && cargo release publish --execute

.PHONY: build
build:  ## build
	pushd $(pkg_src) && cargo build --release

#.PHONY: install
#install: uninstall  ## install
	#@cp -vf $(pkg_src)/target/release/$(BINARY) ~/bin/$(BINARY)
.PHONY: install
install: uninstall  ## install
	@VERSION=$(shell cat VERSION) && \
		echo "-M- Installagin $$VERSION" && \
		cp -vf rsenv/target/release/$(BINARY) ~/bin/$(BINARY)$$VERSION && \
		ln -vsf ~/bin/$(BINARY)$$VERSION ~/bin/$(BINARY)


.PHONY: install-runenv
install-runenv: uninstall-runenv  ## install-runenv
	@cp -vf $(app_root)/scripts/rsenv.sh ~/dev/binx/rsenv.sh

.PHONY: uninstall-runenv
uninstall-runenv:  ## uninstall-runenv
	@rm -f ~/dev/binx/rsenv.sh


.PHONY: uninstall
uninstall:  ## uninstall
	-@test -f ~/bin/$(BINARY) && rm -v ~/bin/$(BINARY)

.PHONY: bump-major
bump-major:  check-github-token  ## bump-major, tag and push
	bump-my-version bump --commit --tag major
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: bump-minor
bump-minor:  check-github-token  ## bump-minor, tag and push
	bump-my-version bump --commit --tag minor
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: bump-patch
bump-patch:  check-github-token  ## bump-patch, tag and push
	bump-my-version bump --commit --tag patch
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: create-release
create-release: check-github-token  ## create a release on GitHub via the gh cli
	@if ! command -v gh &>/dev/null; then \
		echo "You do not have the GitHub CLI (gh) installed. Please create the release manually."; \
		exit 1; \
	else \
		echo "Creating GitHub release for v$(VERSION)"; \
		gh release create "v$(VERSION)" --generate-notes --latest; \
	fi

.PHONY: check-github-token
check-github-token:  ## Check if GITHUB_TOKEN is set
	@if [ -z "$$GITHUB_TOKEN" ]; then \
		echo "GITHUB_TOKEN is not set. Please export your GitHub token before running this command."; \
		exit 1; \
	fi
	@echo "GITHUB_TOKEN is set"
	#@$(MAKE) fix-version  # not working: rustrover deleay


.PHONY: fix-version
fix-version:  ## fix-version of Cargo.toml, re-connect with HEAD
	git add bkmr/Cargo.lock
	git commit --amend --no-edit
	git tag -f "v$(VERSION)"
	git push --force-with-lease
	git push --tags --force


.PHONY: style
style:  ## style
	pushd $(pkg_src) && cargo fmt

.PHONY: lint
lint:  ## lint
	pushd $(pkg_src) && cargo clippy

################################################################################
# Clean \
CLEAN:  ## ############################################################

.PHONY: clean
clean:clean-rs  ## clean all
	:

.PHONY: clean-build
clean-build: ## remove build artifacts
	rm -fr build/
	rm -fr dist/
	rm -fr .eggs/
	find . \( -path ./env -o -path ./venv -o -path ./.env -o -path ./.venv \) -prune -o -name '*.egg-info' -exec rm -fr {} +
	find . \( -path ./env -o -path ./venv -o -path ./.env -o -path ./.venv \) -prune -o -name '*.egg' -exec rm -f {} +

.PHONY: clean-pyc
clean-pyc: ## remove Python file artifacts
	find . -name '*.pyc' -exec rm -f {} +
	find . -name '*.pyo' -exec rm -f {} +
	find . -name '*~' -exec rm -f {} +
	find . -name '__pycache__' -exec rm -fr {} +

.PHONY: clean-rs
clean-rs:  ## clean-rs
	pushd $(pkg_src) && cargo clean -v

################################################################################
# Misc \
MISC:  ## ############################################################

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

debug:  ## debug
	@echo "-D- CODE_DIR: $(CODE_DIR)"


.PHONY: list
list: *  ## list
	@echo $^

.PHONY: list2
%: %.md  ## list2
	@echo $^


%-plan:  ## call with: make <whatever>-plan
	@echo $@ : $*
	@echo $@ : $^
