SHELL := /usr/bin/env bash

INSTALL_VARIANT ?= ds
INSTALL_ROOT ?= $(HOME)/.local/share/rzn-python-tools
INSTALL_BIN_DIR ?= $(HOME)/.local/bin
WORKFLOWS_DIR ?= $(HOME)/.rzn/python-tools/workflows

.PHONY: install build-install-artifact release-installers release-plugins release-artifacts release workflows-sync status

install:
	artifact="$$(python3 scripts/build_local_release.py --variant $(INSTALL_VARIANT) --print-artifact-path)"; \
	sh scripts/install_rzn_python_tools.sh \
	  --artifact-path "$$artifact" \
	  --install-root "$(INSTALL_ROOT)" \
	  --bin-dir "$(INSTALL_BIN_DIR)" \
	  --workflows-dir "$(WORKFLOWS_DIR)"

build-install-artifact:
	python3 scripts/build_local_release.py --variant $(INSTALL_VARIANT)

release-installers:
	python3 scripts/build_local_release.py --all-variants

release-plugins:
	bash scripts/build_python_tools_variants_macos_universal.sh

release-artifacts: release-installers release-plugins

release:
ifndef VERSION
	$(error VERSION is required. Use `make release VERSION=0.2.3`)
endif
	python3 scripts/release.py --version "$(VERSION)"

workflows-sync:
	"$(INSTALL_BIN_DIR)/rzn-python-tools" workflows sync --dest "$(WORKFLOWS_DIR)"

status:
	"$(INSTALL_BIN_DIR)/rzn-python-tools" status
