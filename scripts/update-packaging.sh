#!/usr/bin/env bash
# Fill release checksums into packaging/aur/PKGBUILD and packaging/homebrew/ankh.rb.
# Usage: scripts/update-packaging.sh 0.1.0
set -euo pipefail
v="${1:?version}"
repo="KadenThomp36/ankh"
sum() { curl -sL "https://github.com/$repo/releases/download/v$v/ankh-$v-$1.tar.gz.sha256" | awk '{print $1}'; }
x86l=$(sum x86_64-unknown-linux-gnu); armL=$(sum aarch64-unknown-linux-gnu)
x86m=$(sum x86_64-apple-darwin);      armM=$(sum aarch64-apple-darwin)
sed -i -e "s/^pkgver=.*/pkgver=$v/" -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$x86l')/" -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$armL')/" packaging/aur/PKGBUILD
python3 - "$v" "$armM" "$x86m" "$armL" "$x86l" <<'PY'
import sys,re
v,armM,x86m,armL,x86l=sys.argv[1:]
p="packaging/homebrew/ankh.rb"; s=open(p).read()
s=re.sub(r'version "[^"]+"', f'version "{v}"', s)
for tgt,sha in [("aarch64-apple-darwin",armM),("x86_64-apple-darwin",x86m),("aarch64-unknown-linux-gnu",armL),("x86_64-unknown-linux-gnu",x86l)]:
    s=re.sub(rf'({re.escape(tgt)}\.tar\.gz"\n\s*sha256 ")[^"]*"', rf'\g<1>{sha}"', s)
open(p,"w").write(s)
PY
echo "updated packaging for v$v"
