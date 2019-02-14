
all : build

build :
	cargo build

clean :
	cargo clean

doc :
	cargo doc --all-features

check :
	cargo test  --all-features

