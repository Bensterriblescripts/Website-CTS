#!/usr/bin/env bash
set -euo pipefail

src=$(dirname "$(readlink -f "$0")")

if [ "$(id -u)" -ne 0 ]; then
	echo "install.sh must run as root" >&2
	exit 1
fi

if ! command -v systemctl >/dev/null; then
	echo "systemd is required" >&2
	exit 1
fi

if ! command -v caddy >/dev/null; then
	if command -v apt-get >/dev/null; then
		apt-get update
		apt-get install -y caddy
	elif command -v dnf >/dev/null; then
		dnf install -y caddy
	elif command -v zypper >/dev/null; then
		zypper --non-interactive install caddy
	elif command -v pacman >/dev/null; then
		pacman -Sy --noconfirm caddy
	else
		echo "no supported package manager found, install caddy manually" >&2
		exit 1
	fi
fi

install -o root -g root -m 755 "$src/cts" /usr/local/bin/cts

install -d -o root -g root -m 755 /srv/cts
rm -rf /srv/cts/public.new /srv/cts/public.old
install -d -o root -g root -m 755 /srv/cts/public.new
tar -C /srv/cts/public.new --no-same-owner --no-same-permissions -xf "$src/public.tar"
chown -R root:root /srv/cts/public.new
find /srv/cts/public.new -type d -exec chmod 755 {} +
find /srv/cts/public.new -type f -exec chmod 644 {} +
if [ -d /srv/cts/public ]; then
	mv /srv/cts/public /srv/cts/public.old
fi
mv /srv/cts/public.new /srv/cts/public
rm -rf /srv/cts/public.old

install -o root -g root -m 644 "$src/cts.service" /etc/systemd/system/cts.service
install -d -o root -g root -m 755 /etc/caddy
install -o root -g root -m 644 "$src/Caddyfile" /etc/caddy/Caddyfile

systemctl daemon-reload
systemctl enable cts caddy
systemctl restart cts
systemctl restart caddy

if command -v ufw >/dev/null; then
	ufw allow 22/tcp
	ufw allow 80/tcp
	ufw allow 443/tcp
	ufw allow 443/udp
	ufw --force enable
elif systemctl is-active --quiet firewalld; then
	firewall-cmd --permanent --add-service=ssh --add-service=http --add-service=https --add-port=443/udp
	firewall-cmd --reload
fi

echo "cts deployed"
