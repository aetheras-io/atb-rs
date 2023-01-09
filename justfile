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
	cargo build --features fixtures,uuidv0_8
	cargo build --features fixtures,uuidv1
	cargo build --features eventsourcing
	# --all-features won't work now, there can only be one of uuidv0_8 and uuidv1
	# cargo build --all-features
	(cd types && cargo build)
	(cd ext/tokio && cargo build)
	(cd ext/actix && cargo build --features all)
	(cd ext/serde && cargo build)
	(cd graphql && cargo build)
	(cd utils/cli && cargo build)
	(cd utils/fixtures && cargo build --features uuidv0_8)
	(cd utils/fixtures && cargo build --features uuidv1)
	(cd utils/fixtures && cargo build --features jwt,uuidv0_8)
	(cd utils/fixtures && cargo build --features jwt,uuidv1)

tag:
	git tag -a v{{SEM_VER}} -m "v{{SEM_VER}}"

untag:
	git tag -d v{{SEM_VER}}
