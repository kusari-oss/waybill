# syntax=docker/dockerfile:1.7
#
# Production container image for mikebom (issue #234).
#
# This Dockerfile is consumed by `.github/workflows/release.yml`'s
# `publish-container-image` job, which builds multi-arch images
# (linux/amd64 + linux/arm64) via `docker buildx` + QEMU and pushes
# to `ghcr.io/kusari-sandbox/mikebom`.
#
# DESIGN: The image is built from the per-arch release tarballs the
# existing `build-linux-x86_64` + `build-linux-aarch64` jobs upload —
# the same `cross`-compiled binaries that ship to GitHub Releases.
# This guarantees that the binary inside the container is byte-
# identical to the binary in the download-tarball, so consumers
# scanning either get the same SBOM.
#
# The build context is `./image-context/` and contains pre-extracted
# tarball staging directories keyed by arch:
#   image-context/amd64/staging/ = mikebom-${TAG}-x86_64-unknown-linux-gnu/...
#   image-context/arm64/staging/ = mikebom-${TAG}-aarch64-unknown-linux-gnu/...
#
# `TARGETARCH` is auto-set by buildx per platform (`amd64` /
# `arm64`); the COPY layer below resolves the correct per-arch
# staging directory at build time.
#
# BASE IMAGE: gcr.io/distroless/cc-debian12:nonroot
#  - libc + libssl + ca-certificates (everything `mikebom sbom scan`
#    needs at runtime: TLS roots for deps.dev / registry pulls,
#    glibc for the cross-compiled binary)
#  - no shell, no package manager → minimal attack surface
#  - nonroot user (uid 65532) → Pod Security Standards "restricted"
#    profile compatible
#  - ~25 MB final image
#
# TRACE MODE: `mikebom trace` (eBPF build-time capture, Linux-only)
# requires CAP_BPF + CAP_PERFMON. The image ships the eBPF object
# at the loader's expected relative path
# (`mikebom-ebpf/target/bpfel-unknown-none/release/mikebom-ebpf`)
# and sets WORKDIR=/mikebom so the relative path resolves. To use
# trace mode in a Kubernetes pod, the pod spec needs the
# capabilities + a hostpid mount; for the common `sbom scan` and
# `sbom generate` paths, no special privileges are needed and the
# nonroot user is sufficient.

FROM gcr.io/distroless/cc-debian12:nonroot

# TARGETARCH is auto-populated by buildx with the per-platform value
# (`amd64` / `arm64`) when building multi-arch with
# `--platform linux/amd64,linux/arm64`. Declaring it AFTER FROM lets
# buildx populate it for this stage.
#
# DO NOT also declare it BEFORE FROM. A pre-FROM ARG creates a global
# scope with no value (we never pass --build-arg TARGETARCH), and the
# post-FROM `ARG TARGETARCH` would inherit that empty value rather
# than buildx's auto-set platform value, leaving `${TARGETARCH}`
# empty in the COPY below.
ARG TARGETARCH

# Copy the per-arch tarball staging directory into /mikebom. The
# staging dir already contains the binary, the eBPF object at the
# loader's expected path, README, LICENSE, and the wrapper script
# (`mikebom.sh`) — same layout the release tarball ships.
COPY ${TARGETARCH}/staging/ /mikebom/

# WORKDIR=/mikebom so trace mode finds the eBPF object at its
# expected CWD-relative path. Doesn't affect `sbom scan` or
# `sbom generate` which never invoke the loader.
WORKDIR /mikebom

# Distroless's nonroot user is uid 65532. Explicit USER directive
# documents this for Pod Security Standards conformance checks.
USER nonroot

# Entry directly at the binary (not the wrapper script) — distroless
# has no shell, so `mikebom.sh` can't run anyway. The wrapper exists
# in the staging dir for users who download the tarball and want to
# preserve relative-path semantics; the container image bypasses it.
ENTRYPOINT ["/mikebom/mikebom"]
