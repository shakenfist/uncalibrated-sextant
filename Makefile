.PHONY: build clean qemu spice spice-ryll release release-verify screenshot vendor-probes

BINARY := target/x86_64-unknown-uefi/release/uncalibrated-sextant.efi

build:
	./scripts/build.sh

qemu: build
	./scripts/mkesp.sh
	./scripts/qemu.sh dist/esp.img

spice: build
	./scripts/mkesp.sh
	./scripts/spice.sh dist/esp.img

spice-ryll: build
	./scripts/mkesp.sh
	./scripts/spice-ryll.sh dist/esp.img

release: build
	./scripts/mkesp.sh
	cp dist/esp.img dist/uncalibrated-sextant.img
	qemu-img convert -f raw -O qcow2 dist/uncalibrated-sextant.img dist/uncalibrated-sextant.qcow2
	ls -lh dist/uncalibrated-sextant.img dist/uncalibrated-sextant.qcow2

release-verify: release
	./scripts/verify-release.sh dist/uncalibrated-sextant.img raw
	./scripts/verify-release.sh dist/uncalibrated-sextant.qcow2 qcow2

screenshot: build
	./scripts/screenshot.sh

vendor-probes:
	./scripts/vendor-language-probes.py > src/probes.rs

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target dist
