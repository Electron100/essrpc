
all : build

build :
	cargo build

clean :
	cargo clean

doc :
	cargo doc --all-features

check :
	cargo clippy
	cargo test  --all-features

