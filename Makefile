.PHONY: build clean qemu

BINARY := target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi

build:
	./scripts/build.sh

qemu: build
	./scripts/mkesp.sh
	./scripts/qemu.sh dist/esp.img

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target dist
