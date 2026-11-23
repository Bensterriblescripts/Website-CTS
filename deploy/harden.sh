#!/usr/bin/env bash
set -euo pipefail

proxy=192.168.20.200
app_port=9050
admin_net=192.168.1.0/24
gateway=192.168.5.1
admin_user=ben

if [ "$(id -u)" -ne 0 ]; then
	echo "harden.sh must run as root" >&2
	exit 1
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get -y upgrade
apt-get install -y ufw fail2ban python3-systemd unattended-upgrades apt-listchanges needrestart apparmor apparmor-utils systemd-timesyncd
apt-get purge -y git git-man
apt-get autoremove --purge -y

timedatectl set-timezone Etc/UTC
systemctl enable --now systemd-timesyncd
timedatectl set-ntp true

cat > /etc/apt/apt.conf.d/20auto-upgrades <<'EOF'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
EOF
systemctl enable --now unattended-upgrades apt-daily.timer apt-daily-upgrade.timer

chmod 700 "/home/$admin_user/.ssh"
chmod 600 "/home/$admin_user/.ssh/authorized_keys"
chown -R "$admin_user:$admin_user" "/home/$admin_user/.ssh"
rm -f "/home/$admin_user/.ssh/.authorized_keys.swp"

cat > /etc/ssh/sshd_config.d/10-hardening.conf <<EOF
PermitRootLogin no
PasswordAuthentication no
KbdInteractiveAuthentication no
X11Forwarding no
AllowAgentForwarding no
AllowTcpForwarding no
AllowStreamLocalForwarding no
MaxAuthTries 3
AllowUsers $admin_user
EOF
chmod 644 /etc/ssh/sshd_config.d/10-hardening.conf
sshd -t
systemctl reload ssh

cat > /etc/sysctl.d/99-hardening.conf <<'EOF'
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.default.send_redirects = 0
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv4.conf.all.secure_redirects = 0
net.ipv4.conf.default.secure_redirects = 0
net.ipv6.conf.all.accept_redirects = 0
net.ipv6.conf.default.accept_redirects = 0
net.ipv4.conf.all.accept_source_route = 0
net.ipv4.conf.default.accept_source_route = 0
net.ipv6.conf.all.accept_source_route = 0
net.ipv6.conf.default.accept_source_route = 0
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1
net.ipv4.conf.all.log_martians = 1
net.ipv4.conf.default.log_martians = 1
net.ipv4.icmp_echo_ignore_broadcasts = 1
net.ipv4.icmp_ignore_bogus_error_responses = 1
net.ipv4.tcp_syncookies = 1
net.ipv4.tcp_rfc1337 = 1
kernel.kptr_restrict = 2
kernel.dmesg_restrict = 1
kernel.yama.ptrace_scope = 1
kernel.unprivileged_bpf_disabled = 1
net.core.bpf_jit_harden = 2
kernel.randomize_va_space = 2
fs.protected_hardlinks = 1
fs.protected_symlinks = 1
fs.protected_fifos = 2
fs.protected_regular = 2
fs.suid_dumpable = 0
EOF
chmod 644 /etc/sysctl.d/99-hardening.conf
sysctl --system >/dev/null

cat > /etc/fail2ban/jail.local <<'EOF'
[DEFAULT]
bantime = 30m
findtime = 10m
maxretry = 3

[sshd]
enabled = true
backend = systemd
EOF
chmod 644 /etc/fail2ban/jail.local
systemctl enable fail2ban
systemctl restart fail2ban

ufw --force reset
ufw default deny incoming
ufw default allow outgoing
ufw default deny routed
ufw allow from "$admin_net" to any port 22 proto tcp
ufw allow from "$proxy" to any port "$app_port" proto tcp
ufw allow out to "$gateway" port 53 proto udp
ufw allow out to "$gateway" port 53 proto tcp
ufw allow out to "$gateway" port 67 proto udp
ufw allow out to "$gateway" port 123 proto udp
ufw deny out to 10.0.0.0/8
ufw deny out to 172.16.0.0/12
ufw deny out to 192.168.0.0/16
ufw deny out to 169.254.0.0/16
ufw deny out to 100.64.0.0/10
ufw deny out to fc00::/7
ufw logging low
ufw --force enable
systemctl enable ufw

echo "cts hardened"
