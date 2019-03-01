
all : build

build :
	cargo build

clean :
	cargo clean

doc :
	cargo doc --all-features

check :
	cargo clippy --all-features -- -D warnings
	cargo test  --all-features

