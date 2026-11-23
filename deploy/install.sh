#!/usr/bin/env bash
set -euo pipefail

src=$(dirname "$(readlink -f "$0")")

if [ "$(id -u)" -ne 0 ]; then
	echo "install.sh must run as root" >&2
	exit 1
fi

install -d -o root -g root -m 755 /srv/cts
rm -rf /srv/cts/public.new /srv/cts/public.old
install -d -o root -g root -m 755 /srv/cts/public.new
tar -C /srv/cts/public.new --no-same-owner --no-same-permissions -xf "$src/public.tar"
CTS_PUBLIC=/srv/cts/public.new RUST_LOG=warn "$src/cts" --precompress
chown -R root:root /srv/cts/public.new
find /srv/cts/public.new -type d -exec chmod 755 {} +
find /srv/cts/public.new -type f -exec chmod 644 {} +

install -o root -g root -m 755 "$src/cts" /usr/local/bin/cts
install -o root -g root -m 644 "$src/cts.service" /etc/systemd/system/cts.service

if [ -d /srv/cts/public ]; then
	mv /srv/cts/public /srv/cts/public.old
fi
mv /srv/cts/public.new /srv/cts/public
rm -rf /srv/cts/public.old

systemctl daemon-reload
systemctl enable cts
systemctl restart cts
sleep 1
systemctl is-active --quiet cts

echo "cts deployed"
