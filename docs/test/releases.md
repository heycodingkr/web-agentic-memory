# CLI packaging and signed archive verification

GitHub Actions workflows were removed on 2026-10-30 at the user's request. This checkout does not automatically build, sign, or publish binary releases. To use WAM now, follow the [source installation instructions](../../README.md#run-from-this-checkout).

The local packaging tools support `wam` and `wam-mcp` archives for Linux x86_64/ARM64, macOS Intel/ARM64, and Windows x86_64. They check the tag against both binary package versions, embed the full source commit in each archive, require all five targets when verifying a complete release, and generate `SHA256SUMS`. Target support in these tools does not establish native runtime acceptance on every OS. See [release acceptance](release-readiness.md).

## Package a local build

For example, from a checkout of the source commit being packaged:

```sh
cargo build --locked --release -p wam-cli -p wam-mcp
python3 tools/release/package.py build \
  --tag v0.1.0 --target aarch64-apple-darwin \
  --source-commit "$(git rev-parse HEAD)" \
  --binary-dir target/release --output-dir dist --workspace . --smoke
```

Choose the actual native target and matching package version for the machine and checkout. This creates a local archive; it supplies no signature or published release. The downloaded-archive installer below rejects unsigned output.

## Verify an existing signed archive

The offline installer requires Python 3.11+ and Cosign on PATH, plus an archive, `SHA256SUMS`, and a `.sigstore.json` bundle for each. It retains its exact GitHub OIDC certificate identity check for archives produced by the former release signing process. Removing the workflow does not add a signature bypass or create a replacement signing service.

Choose the expected repository and workflow branch independently of the downloaded files. For a Linux x86_64 artifact:

```sh
python3 tools/release/install.py \
  --archive wam-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --checksums SHA256SUMS --version 0.1.0 \
  --target x86_64-unknown-linux-gnu \
  --repository '<owner>/<repo>' --workflow-ref refs/heads/main
```

The default verifies without installing or running binaries. The installer verifies both signatures against the exact certificate identity and GitHub OIDC issuer, then checks the archive hash and embedded version/target/source commit. It stages bounded copies before verification, rejects links, traversal, duplicate/unexpected members and corrupt metadata, and exposes no signature bypass flag.

To install a verified archive, add `--prefix /absolute/path/to/a/new/wam-directory`. The target must match the host, and the directory must not already exist. Existing installations are never overwritten. Add that directory to PATH, check `wam --version`, then register `wam-mcp` using the web guide and inspect its tools from the host. Keep the `.wam-install.json` receipt. Remove that dedicated directory to uninstall the downloaded binaries; host registrations and account credentials have separate removal steps in the web guide.

## Native source-install acceptance

Native macOS ARM64 source-install acceptance on 2026-10-30 passed with Codex CLI 0.165.0 and Claude Code 2.1.314: actual installed CLI status, stdio initialize/list of ten tools/positive recall/EOF, idempotent registration/removal and preservation of another owned server. Evidence is `target/e2e-logs/native-install/20261030-031817-302b/`. Dependencies and release builds were cached; the measured install/first-use times are not a clean-machine download/compile benchmark. The separate native macOS Keychain dummy-entry round trip also passed and removed its entry (`target/release-verification/macos-keychain.log`). Both checks use disposable namespaces. They do not verify a signed archive, GitHub authorization, native Windows/Linux, or production operation.

Reproduce with `python3 -B tests/env/verify_native_install.py` and `cargo test --locked -p wam-client --test secure_store -- --ignored --nocapture`. See [environment requirements and scope](../../tests/env/README.md#native-release-rehearsal).

## Local regression

```sh
python3 -B -m unittest discover -s tests/lib/selftest -t tests/lib -p 'test_release_*.py' -v
```

Regression covers package metadata/checksums, required targets, staged installation, integrity failures, rejected archive entries, existing-directory preservation and the real CLI's fail-closed signature boundary. Positive installation tests inject a test verifier; they do not establish a real signature or a published artifact. Native Windows/Linux runtime, Apple Developer ID signing/notarization, Windows Authenticode, license and reference-rights decisions remain separate release requirements.
