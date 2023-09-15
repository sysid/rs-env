.DEFAULT_GOAL := help
#MAKEFLAGS += --no-print-directory

# You can set these variables from the command line, and also from the environment for the first two.
PREFIX ?= /usr/local
BINPREFIX ?= "$(PREFIX)/bin"

VERSION       = $(shell cat VERSION)

SHELL	= bash
.ONESHELL:

app_root = .
app_root ?= .
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
	rm -fr ~/xxx/*
	mkdir -p ~/xxx
	cp -r $(tests_src)/resources/environments/complex/dot.envrc ~/xxx/.envrc
	cat ~/xxx/.envrc

.PHONY: show-env
show-env:  ## show-env
	@tree -a ~/xxx


.PHONY: test
test:  ## test
	RUST_LOG=DEBUG pushd $(pkg_src) && cargo test -- --test-threads=1  # --nocapture
	#RUST_LOG=DEBUG pushd $(pkg_src) && cargo test

.PHONY: run-leaves
run-leaves:  ## run-leaves
	pushd $(pkg_src) && cargo run -- leaves tests/resources/environments/tree2/confguard

.PHONY: run-edit
run-edit:  ## run-edit
	pushd $(pkg_src) && cargo run -- edit ./tests/resources/environments/complex

.PHONY: run-build
run-build:  ## run-build
	pushd $(pkg_src) && time cargo run -- -d -d build ./tests/resources/environments/complex/level4.env

.PHONY: run-select
run-select:  ## run-select
	rsenv/target/debug/rsenv select $(SOPS_PATH)
	cat .envrc

.PHONY: run-files
run-files:  ## run-files
	pushd $(pkg_src) && time cargo run -- -d -d files ./tests/resources/environments/complex/level4.env

.PHONY: run-envrc
run-envrc: init-env  ## run-envrc
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
	pushd $(pkg_src) && cargo release publish --execute

.PHONY: build
build:  ## build
	pushd $(pkg_src) && cargo build --release

.PHONY: install
install: uninstall  ## install
	@cp -vf $(pkg_src)/target/release/$(BINARY) ~/bin/$(BINARY)

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
bump-major:  ## bump-major, tag and push
	bumpversion --commit --tag major
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: bump-minor
bump-minor:  ## bump-minor, tag and push
	bumpversion --commit --tag minor
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: bump-patch
bump-patch:  ## bump-patch, tag and push
	bumpversion --commit --tag patch
	git push
	git push --tags
	@$(MAKE) create-release

.PHONY: style
style:  ## style
	pushd $(pkg_src) && cargo fmt

.PHONY: lint
lint:  ## lint
	pushd $(pkg_src) && cargo clippy

.PHONY: create-release
create-release:  ## create a release on GitHub via the gh cli
	@if command -v gh version &>/dev/null; then \
		echo "Creating GitHub release for v$(VERSION)"; \
		gh release create "v$(VERSION)" --generate-notes; \
	else \
		echo "You do not have the github-cli installed. Please create release from the repo manually."; \
		exit 1; \
	fi


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
