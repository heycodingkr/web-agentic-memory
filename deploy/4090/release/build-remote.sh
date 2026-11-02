#!/usr/bin/env bash
# Linux build and verification of a pinned source tree on the production host's builder. Nothing is activated here.
set -euo pipefail
release="$1"
work="/srv/web-agentic-memory/builds/$release"
builder="rust@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97"
db="wam-deploy-test-$release"
cleanup() { sudo -n docker rm -f "$db" >/dev/null 2>&1 || true; }
trap cleanup EXIT
mkdir -p "$work/source"
tar xzf "$work/source.tar.gz" -C "$work/source"
if [ ! -e "$work/target" ]; then ln -s /srv/web-agentic-memory/builds/20261101T075251Z-admin/target "$work/target"; fi
sudo -n docker run -d --name "$db" -e POSTGRES_PASSWORD=wam-disposable-deploy-test -p 127.0.0.1::5432 postgres:16 >/dev/null
for attempt in $(seq 1 30); do
  if sudo -n docker exec "$db" pg_isready -U postgres >/dev/null 2>&1; then break; fi
  sleep 1
done
port=$(sudo -n docker inspect "$db" --format '{{(index (index .NetworkSettings.Ports "5432/tcp") 0).HostPort}}')
sudo -n docker run --rm --name "wam-deploy-builder-$release" --network host --user "$(id -u):$(id -g)" \
  -e CARGO_BUILD_JOBS=6 -e CARGO_PROFILE_DEV_DEBUG=0 \
  -e WAM_TEST_DATABASE_URL="postgres://postgres:wam-disposable-deploy-test@127.0.0.1:$port/postgres" \
  -v "$work/source:/work" -v "$work/target:/work/target" \
  -v "$HOME/.cargo/registry:/usr/local/cargo/registry" -w /work "$builder" bash -c \
  'cargo test --locked -p wam-server -p wam-gatekeeper -p wam-core -p wam-contracts --no-fail-fast 2>&1 | grep -E "^(test result|error|failures:|---- |test .* FAILED)|Running" ;
   test ${PIPESTATUS[0]} -eq 0 &&
   cargo test --locked -p wam-server --test postgres_application -- --ignored --test-threads=1 &&
   cargo test --locked -p wam-server --test email_auth_postgres -- --ignored --test-threads=1 &&
   cargo build --locked --release -p wam-server --bin wam-server'
mkdir -p "$work/bin"
install -m 755 "$work/target/release/wam-server" "$work/bin/wam-server"
sha256sum "$work/bin/wam-server" | tee "$work/bin.sha256"
printf 'DEPLOY_BUILD_AND_TESTS_PASSED\n'
