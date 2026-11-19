#!/usr/bin/env bash
set -euo pipefail
version=${1:-0.3.0}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

rpm_sources=$(mktemp -d)
trap 'rm -rf "$rpm_sources"' EXIT

cp target/release/douri "$rpm_sources/"
cp dist/linux/io.github.triadwoozie.douri.desktop "$rpm_sources/"
cp dist/linux/io.github.triadwoozie.douri.png "$rpm_sources/"
cp dist/linux/douri-mime.xml "$rpm_sources/"
cp dist/README-linux.txt "$rpm_sources/README.txt"

rpmbuild -bb dist/linux/douri.spec \
  --define "_sourcedir $rpm_sources" \
  --define "pkg_version $version" \
  --define "_rpmdir $root/dist"

if [ -f "dist/x86_64/douri-${version}-1.x86_64.rpm" ]; then
  mv "dist/x86_64/douri-${version}-1.x86_64.rpm" "dist/douri-${version}-1.x86_64.rpm"
  rmdir dist/x86_64 2>/dev/null || true
fi
echo "RPM built at: dist/douri-${version}-1.x86_64.rpm"
