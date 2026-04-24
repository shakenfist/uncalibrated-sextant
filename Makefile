.PHONY: build clean

BINARY := target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi

build:
	./scripts/build.sh

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target
