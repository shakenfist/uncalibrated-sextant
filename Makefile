.PHONY: build clean qemu release release-verify

BINARY := target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi

build:
	./scripts/build.sh

qemu: build
	./scripts/mkesp.sh
	./scripts/qemu.sh dist/esp.img

release: build
	./scripts/mkesp.sh
	cp dist/esp.img dist/uncalibrated-sextant.img
	qemu-img convert -f raw -O qcow2 dist/uncalibrated-sextant.img dist/uncalibrated-sextant.qcow2
	ls -lh dist/uncalibrated-sextant.img dist/uncalibrated-sextant.qcow2

release-verify: release
	./scripts/verify-release.sh dist/uncalibrated-sextant.img raw
	./scripts/verify-release.sh dist/uncalibrated-sextant.qcow2 qcow2

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target dist
