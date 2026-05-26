.PHONY: build clean qemu spice spice-ryll release release-verify screenshot screenshot-modes vendor-probes digest-smoke digest-payload-smoke

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

screenshot-modes: build
	./scripts/mkesp.sh
	./scripts/screenshot-modes.sh

# Headless QR round-trip smoke. Rebuilds with the digest-smoke cargo
# feature (which injects a hard-coded draw_digest(b"hello") into
# run_awaiting), boots QEMU headless, screendumps to PNG, and uses
# zbarimg to decode + assert the payload. Requires `zbar-tools`
# (apt install zbar-tools) on the host. Invokes cargo inside the
# build container directly because scripts/build.sh does not yet
# accept a --features pass-through; the production build path stays
# on the wrapper.
digest-smoke:
	docker build -t uncalibrated-sextant-build:1.88.0 .
	docker run --rm \
	    -v "$(CURDIR)":/work \
	    -v uncalibrated-sextant-target:/work/target \
	    -w /work \
	    uncalibrated-sextant-build:1.88.0 \
	    cargo build --release --features digest-smoke
	./scripts/digest-smoke.sh

# Full-scene companion to digest-smoke. Where digest-smoke holds in
# AWAITING and verifies the QR round-trip, this target drives the
# scripted scene through to parking, screendumps, decodes, and asserts
# a richer set of TLV invariants (magic + version + frame counter >= 3
# + record count >= 1 + per-record tag validation). Surfaces the
# trailing framebuffer CRC32C in the success line.
digest-payload-smoke:
	docker build -t uncalibrated-sextant-build:1.88.0 .
	docker run --rm \
	    -v "$(CURDIR)":/work \
	    -v uncalibrated-sextant-target:/work/target \
	    -w /work \
	    uncalibrated-sextant-build:1.88.0 \
	    cargo build --release --features digest-smoke
	./scripts/digest-payload-smoke.sh

vendor-probes:
	./scripts/vendor-language-probes.py > src/probes.rs

clean:
	docker volume rm -f uncalibrated-sextant-target
	rm -rf target dist
