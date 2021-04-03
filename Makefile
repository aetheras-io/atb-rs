# Path and Variables
SHELL := /bin/bash
ORG := aetheras-io
PROJECT := atb-rs 
REPO := github.com/${ORG}/${PROJECT}
ROOT_DIR := $(CURDIR)
SEM_VER := $(shell awk -F' = ' '$$1=="version"{print $$2;exit;}' ./Cargo.toml)

sanity-build: 
	cargo build --features http 
	cargo build --features sql 
	cargo build --features graphql 
	cargo build --features "sql graphql" 
	cargo build --features fixtures 
	cargo build --features jwt 
	cargo build --features serde_utils 
	cargo build --all-features

tag:
	git tag -a v${SEM_VER} -m "v${SEM_VER}"

untag:
	git tag -d v${SEM_VER}
