# Path and Variables
SHELL := /bin/bash
ORG := aetheras-io
PROJECT := atb-rs 
REPO := github.com/${ORG}/${PROJECT}
ROOT_DIR := $(CURDIR)
SEM_VER := $(shell awk -F' = ' '$$1=="version"{print $$2;exit;}' ./Cargo.toml)

build: 
	cargo build --features fixtures 
	cargo build --features eventsourcing 
	cargo build --all-features
	(cd types && cargo build)
	(cd tokio-ext && cargo build)
	(cd actix-ext && cargo build)
	(cd graphql && cargo build)
	(cd utils/cli && cargo build)
	(cd utils/serde-ext && cargo build)

tag:
	git tag -a v${SEM_VER} -m "v${SEM_VER}"

untag:
	git tag -d v${SEM_VER}
