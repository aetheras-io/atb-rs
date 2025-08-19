# Path and Variables
ORG := "aetheras-io"
PROJECT := "atb-rs"
REPO := "https://github.com" / ORG / PROJECT
ROOT_DIR := justfile_directory()
SEM_VER := `awk -F' = ' '$1=="version"{print $2;exit;}' ./Cargo.toml`

default:
    @just --choose

semver:
	@echo {{SEM_VER}}

# #FIXME this build step isn't running properly
build: 
	cargo build --features fixtures 
	cargo build --features eventsourcing
	cargo build --all-features
	(cd types && cargo build)
	(cd ext/tokio && cargo build)
	(cd ext/actix && cargo build --features all)
	(cd ext/serde && cargo build)
	(cd utils/cli && cargo build)
	(cd utils/fixtures && cargo build)
	(cd utils/fixtures && cargo build --features jwt) && cargo clippy --all

tag:
	git tag -a v{{SEM_VER}} -m "v{{SEM_VER}}"

untag:
	git tag -d v{{SEM_VER}}
