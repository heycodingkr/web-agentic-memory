# Signed CLI release draft

`.github/workflows/release.yml` builds the `wam` and `wam-mcp` release binaries for Linux x86_64/ARM64, macOS Intel/ARM64, and Windows x86_64. Dispatch it from the repository default branch with an existing `vMAJOR.MINOR.PATCH` tag. The tag commit must be reachable from the default branch. The workflow runs trusted packaging/signing tools from the workflow commit; it builds the tag's source code in a read-only build job, embeds the source commit in each archive, smoke-tests `wam --help`, verifies that all five target archives match the tag and source commit, and creates one aggregate `SHA256SUMS` file.

Linux ARM64 uses the native `ubuntu-24.04-arm` runner listed in [GitHub's runner documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job). The packaging/installer and both workflow matrices include it. Static/local tests establish target consistency, not that a GitHub ARM64 job or native secure-store test has run. Public-release acceptance is tracked by [the release gate contract](release-readiness.md).

The signing job uses GitHub Actions OIDC with Cosign keyless blob signing. It signs each archive and `SHA256SUMS`, verifies each bundle against the workflow identity on the repository default branch and GitHub's OIDC issuer, then creates a **draft** GitHub release. A maintainer must review and publish the draft. The job never replaces an existing release for the tag. The tag checkout is used only to resolve its Git commit; no code from it runs in the signing job.

This is a signed provenance/integrity bundle, not Apple Developer ID signing/notarization or Windows Authenticode. Those platform signatures and runtime checks remain separate. This repository currently has no root license file and its open-source readiness checklist still requires a license and reference-rights review, so the release workflow intentionally creates drafts only.

## Run and verify

Dispatch the workflow from the repository default branch, for example:

```sh
gh workflow run release.yml --ref main -f tag=v0.1.0
```

Download the archive, `SHA256SUMS`, and the `.sigstore.json` bundle for each. The offline installer requires Python 3.11+ and Cosign on PATH. Choose the expected repository and workflow branch independently of those downloaded files. For a Linux x86_64 artifact:

```sh
python3 tools/release/install.py \
  --archive wam-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --checksums SHA256SUMS --version 0.1.0 \
  --target x86_64-unknown-linux-gnu \
  --repository '<owner>/<repo>' --workflow-ref refs/heads/main
```

The default verifies without installing or running binaries. The installer verifies both signatures against the exact workflow certificate identity and GitHub OIDC issuer, then checks the archive hash and embedded version/target/source commit. It stages bounded copies before verification, rejects links, traversal, duplicate/unexpected members and corrupt metadata, and exposes no signature bypass flag.

To install, add `--prefix /absolute/path/to/a/new/wam-directory`. The target must match the host, and the directory must not already exist. Existing installations are never overwritten. Add that directory to PATH yourself, check `wam --version`, then register `wam-mcp` using the web guide and inspect its tools from the host. Keep the `.wam-install.json` receipt. Remove that dedicated directory to uninstall the downloaded binaries; host registrations and account credentials have separate removal steps in the web guide.

Local regression covers package metadata/checksums, staged installation, integrity failures, rejected archive entries, existing-directory preservation and the real CLI's fail-closed signature boundary:

```sh
python3 -B -m unittest tests/lib/selftest/test_release_packaging.py tests/lib/selftest/test_release_install.py -v
```

Positive installation tests inject a test verifier; they do not establish a GitHub OIDC signature. Verification against an actual signed GitHub draft still needs repository authentication and an existing release tag. The YAML and packaging logic can be tested locally, but local execution cannot produce a GitHub OIDC identity or validate GitHub release permissions. Native Windows/Linux execution and platform signing/notarization remain separate.
