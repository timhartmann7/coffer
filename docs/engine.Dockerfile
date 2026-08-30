# The engine job's machine, near enough to be evidence: the same distribution
# as ubuntu-latest, so `apt-get install keepassxc` gives the round-trip suite
# the same outside implementation CI compares against.
#
# What this is for is in vault-core.md, under "Compiling it for the machine CI
# runs on". Three Linux-only failures have reached CI green from a Mac, and
# none of them was a compile error.
FROM ubuntu:24.04

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      keepassxc ca-certificates curl build-essential pkg-config \
 && rm -rf /var/lib/apt/lists/*

# Not root. `permissions_apply()` is `geteuid() != 0`, and the three tests that
# ask it return early when it is false - so a container running as root reports
# them passed having run none of them.
RUN useradd --create-home --uid 1001 runner
USER runner
ENV HOME=/home/runner
ENV PATH=/home/runner/.cargo/bin:$PATH

RUN curl -fsSL https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain stable

# Its own target directory, because the host triple here is the same one the
# documented cross-check writes to and a shared tree would have the two
# overwrite each other's artefacts.
#
# Made here, as `runner`, rather than left to the mount: a named volume takes
# its ownership from the image, and one that finds nothing at the path is
# created owned by root, which this user then cannot write to.
ENV CARGO_TARGET_DIR=/home/runner/target
RUN mkdir -p /home/runner/target
WORKDIR /src
