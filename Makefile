
all : build

build :
	cargo build

clean :
	cargo clean

check :
	cargo test  --all-features

