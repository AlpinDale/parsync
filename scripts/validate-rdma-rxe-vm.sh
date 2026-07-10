#!/usr/bin/env bash
set -euo pipefail

repo="$(pwd -P)"
if [[ ! -f "$repo/Cargo.toml" ]]; then
  echo "run from the parsync repository root" >&2
  exit 2
fi

cargo build

inner="$(mktemp /tmp/parsync-rdma-rxe-vm.XXXXXX.sh)"
trap 'rm -f "$inner"' EXIT

cat > "$inner" <<VM
#!/usr/bin/env bash
set -euo pipefail

cd $(printf '%q' "$repo")

echo "[vm] kernel: \$(uname -a)"
echo "[vm] interfaces:"
ip -brief addr

echo "[vm] loading software RDMA module"
modprobe rdma_rxe

ip link set eth0 up
local_ip="\$(ip -4 -o addr show dev eth0 | awk '{sub(/\\/.*/, "", \$4); print \$4; exit}')"
if [[ -z "\$local_ip" ]]; then
  ip addr add 10.0.2.15/24 dev eth0 || true
  local_ip=10.0.2.15
fi

rdma link add rxe-parsync type rxe netdev eth0
echo "[vm] RDMA links:"
rdma link show
ibv_devices

test -d /sys/class/infiniband
ldconfig -p | awk '/librdmacm/ {found=1} END {exit !found}'

echo "[vm] RDMA bind IP: \$local_ip"

work=/tmp/parsync-rdma-validate
rm -rf "\$work"
mkdir -p "\$work" /run/sshd /root/.ssh
chmod 700 /root/.ssh
awk -F: 'BEGIN{OFS=FS} /^root:/ {\$2=""} {print}' /etc/shadow > "\$work/shadow"
chmod 600 "\$work/shadow"
mount --bind "\$work/shadow" /etc/shadow

ssh-keygen -q -N '' -t ed25519 -f "\$work/client_key"
ssh-keygen -q -N '' -t ed25519 -f "\$work/host_key"

cat > /root/.ssh/config <<EOF
Host \$local_ip
  HostName \$local_ip
  Port 2222
  User root
  IdentityFile \$work/client_key
EOF
chmod 600 /root/.ssh/config

cat > "\$work/sshd_config" <<EOF
Port 2222
ListenAddress \$local_ip
HostKey \$work/host_key
PidFile \$work/sshd.pid
AuthorizedKeysFile \$work/client_key.pub
AllowUsers root
PermitRootLogin yes
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
UsePAM no
StrictModes no
Subsystem sftp internal-sftp
EOF

/usr/bin/sshd -f "\$work/sshd_config" -E "\$work/sshd.log"
trap 'cat "\$work/sshd.log" >&2 || true; test ! -f "\$work/sshd.pid" || kill "\$(cat "\$work/sshd.pid")" 2>/dev/null || true' EXIT

src=/tmp/parsync-rdma-source.bin
dst=/tmp/parsync-rdma-dest
rm -rf "\$src" "\$dst"
python3 - <<'PY'
from pathlib import Path
data = (b'parsync-rdma-validation\\n' * 4096) + bytes(range(256))
Path('/tmp/parsync-rdma-source.bin').write_bytes(data)
PY
expected="\$(sha256sum "\$src" | awk '{print \$1}')"

echo "[vm] running parsync RDMA-required transfer"
set +e
target/debug/parsync \\
  --debug \\
  --rdma=require \\
  --rdma-bind "\$local_ip" \\
  --rdma-min-size 1 \\
  --rdma-helper "$repo/target/debug/parsync --internal-rdma-send" \\
  "root@\$local_ip:2222:\$src" \\
  "\$dst" \\
  2> "\$work/parsync.stderr"
parsync_status=\$?
set -e

cat "\$work/parsync.stderr"
if [[ \$parsync_status -ne 0 ]]; then
  exit "\$parsync_status"
fi
actual="\$(sha256sum "\$dst/\$(basename "\$src")" | awk '{print \$1}')"
test "\$actual" = "\$expected"
grep -q 'rdma_files=1' "\$work/parsync.stderr"
grep -q 'rdma_fallbacks=0' "\$work/parsync.stderr"

echo "[vm] RDMA validation passed: \$actual"
VM

chmod +x "$inner"
vng --run --disable-kvm --memory "${PARSYNC_RDMA_VM_MEMORY:-1G}" --cpus "${PARSYNC_RDMA_VM_CPUS:-2}" --network user --exec "bash $inner"
