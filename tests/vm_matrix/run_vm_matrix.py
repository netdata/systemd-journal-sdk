#!/usr/bin/env python3
"""SOW-0075 VM historical systemd validation harness.

The durable outputs are sanitized reports only. Raw VM journals and operational
state stay under `.local/sow-0075/` and must not be staged.
"""

from __future__ import annotations

import argparse
import base64
import dataclasses
import hashlib
import json
import os
import shutil
import subprocess  # nosec B404
import sys
import time
from pathlib import Path
from typing import Any
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from tests.corpus_eval.canonical import SCHEMA_VERSION, digest_export_stream


SCHEMA = "systemd-journal-sdk-vm-matrix-v1"
LOCAL = ROOT / ".local" / "sow-0075"
RAW_DIR = LOCAL / "raw"
STATE_DIR = LOCAL / "state"
BIN_DIR = LOCAL / "bin"
KNOWN_HOSTS = LOCAL / "known_hosts"
IMAGE_CACHE = Path(os.environ.get("SOW_0075_IMAGE_CACHE", str(LOCAL / "images")))
SEED_WORK = Path(os.environ.get("SOW_0075_SEED_WORK", str(LOCAL / "seeds")))
LIBVIRT_IMAGES = Path("/var/lib/libvirt/images")
DISK_CAP_BYTES = 4 * 1024 * 1024 * 1024


@dataclasses.dataclass(frozen=True)
class Target:
    alias: str
    name: str
    distro: str
    expected_systemd: str
    osinfo: str
    image_url: str
    checksum_url: str
    checksum_algorithm: str
    checksum: str
    source_note: str


TARGETS: dict[str, Target] = {
    "ubuntu1804": Target(
        alias="ubuntu1804",
        name="sdjournal-ubuntu1804",
        distro="Ubuntu 18.04 LTS",
        expected_systemd="237-era",
        osinfo="ubuntu18.04",
        image_url="https://cloud-images.ubuntu.com/releases/bionic/release/ubuntu-18.04-server-cloudimg-amd64.img",
        checksum_url="https://cloud-images.ubuntu.com/releases/bionic/release/SHA256SUMS",
        checksum_algorithm="sha256",
        checksum="8dd2e6b5e5aad20c3f836123b300cba9861249408cbb07c359145a65d6bab6b6",
        source_note="official Ubuntu bionic cloud image, final regular-support release build",
    ),
    "debian11": Target(
        alias="debian11",
        name="sdjournal-debian11",
        distro="Debian 11 bullseye",
        expected_systemd="247-era",
        osinfo="debian11",
        image_url="https://cloud.debian.org/images/cloud/bullseye/latest/debian-11-genericcloud-amd64.qcow2",
        checksum_url="https://cloud.debian.org/images/cloud/bullseye/latest/SHA512SUMS",
        checksum_algorithm="sha512",
        checksum="2e9311602ce0d6a7f7e3bdaea03507de99c67aecd1b93563c5a8d5d08d16d224caa3afc867df56977631ea2fec940250a3bcdd16393fee8c4cccd1e8c1e8d3bd",
        source_note="official Debian bullseye genericcloud image",
    ),
    "ubuntu2204": Target(
        alias="ubuntu2204",
        name="sdjournal-ubuntu2204",
        distro="Ubuntu 22.04 LTS",
        expected_systemd="249-era",
        osinfo="ubuntu22.04",
        image_url="https://cloud-images.ubuntu.com/releases/jammy/release/ubuntu-22.04-server-cloudimg-amd64.img",
        checksum_url="https://cloud-images.ubuntu.com/releases/jammy/release/SHA256SUMS",
        checksum_algorithm="sha256",
        checksum="f6729b53d930d7f0c6691eb553cfa6be7109de9412125bf1bf2dc6747de8a44d",
        source_note="official Ubuntu jammy cloud image current release build",
    ),
    "ubuntu2404": Target(
        alias="ubuntu2404",
        name="sdjournal-ubuntu2404",
        distro="Ubuntu 24.04 LTS",
        expected_systemd="255-era",
        osinfo="ubuntu24.04",
        image_url="https://cloud-images.ubuntu.com/releases/noble/release/ubuntu-24.04-server-cloudimg-amd64.img",
        checksum_url="https://cloud-images.ubuntu.com/releases/noble/release/SHA256SUMS",
        checksum_algorithm="sha256",
        checksum="53fdde898feed8b027d94baa9cfe8229867f330a1d9c49dc7d84465ee7f229f7",
        source_note="official Ubuntu noble cloud image current release build",
    ),
}


def command_sha256(cmd: list[str]) -> str:
    return hashlib.sha256(json.dumps(cmd, separators=(",", ":")).encode()).hexdigest()


def run(
    cmd: list[str],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    timeout: int = 300,
    input_bytes: bytes | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[bytes]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    result = subprocess.run(  # nosec B603
        cmd,  # nosemgrep
        cwd=cwd,
        env=env,
        input=input_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    if check and result.returncode != 0:
        raise RuntimeError(
            json.dumps(
                {
                    "returncode": result.returncode,
                    "command_sha256": command_sha256(cmd),
                    "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                    "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
                },
                sort_keys=True,
            )
        )
    return result


def ensure_local_dirs() -> None:
    for path in (LOCAL, RAW_DIR, STATE_DIR, BIN_DIR, IMAGE_CACHE, SEED_WORK):
        path.mkdir(parents=True, exist_ok=True)


def selected_targets(values: list[str]) -> list[Target]:
    if values == ["all"]:
        return list(TARGETS.values())
    result = []
    for value in values:
        if value not in TARGETS:
            raise SystemExit(f"unknown target: {value}")
        result.append(TARGETS[value])
    return result


def which(name: str) -> str | None:
    return shutil.which(name)


def qemu_image_info(url: str) -> dict[str, Any]:
    result = run(["qemu-img", "info", "--output=json", url], timeout=60)
    return json.loads(result.stdout.decode())


def domain_exists(name: str) -> bool:
    result = run(
        ["sudo", "-n", "virsh", "--connect", "qemu:///system", "dominfo", name],
        check=False,
        timeout=30,
    )
    return result.returncode == 0


def check_bridge() -> bool:
    return run(["ip", "-brief", "link", "show", "br0"], check=False).returncode == 0


REQUIRED_PREFLIGHT_TOOLS = ["virsh", "virt-install", "qemu-img", "genisoimage", "ssh", "scp", "curl", "journalctl"]
OPTIONAL_PREFLIGHT_TOOLS = ["cargo", "go", "python3", "node"]


def preflight_tools() -> dict[str, str | None]:
    return {name: which(name) for name in REQUIRED_PREFLIGHT_TOOLS + OPTIONAL_PREFLIGHT_TOOLS}


def missing_preflight_tools(tools: dict[str, str | None]) -> list[str]:
    return [name for name in REQUIRED_PREFLIGHT_TOOLS if tools.get(name) is None]


def target_virtual_size(target: Target) -> tuple[int, list[str]]:
    try:
        return int(qemu_image_info(target.image_url)["virtual-size"]), []
    except Exception:
        return 0, ["IMAGE_INFO_FAILED"]


def preflight_target_row(target: Target) -> dict[str, Any]:
    virtual_size, discrepancies = target_virtual_size(target)
    if domain_exists(target.name):
        discrepancies.append("DOMAIN_EXISTS")
    if virtual_size > DISK_CAP_BYTES:
        discrepancies.append("IMAGE_TOO_LARGE_FOR_CAP")
    return {
        "alias": target.alias,
        "vm_name": target.name,
        "distro": target.distro,
        "expected_systemd": target.expected_systemd,
        "osinfo": target.osinfo,
        "image_url": target.image_url,
        "checksum_url": target.checksum_url,
        "checksum_algorithm": target.checksum_algorithm,
        "checksum": target.checksum,
        "source_note": target.source_note,
        "resources": {"vcpus": 1, "memory_mib": 1024, "disk_gib": 4},
        "image_virtual_size_bytes": virtual_size,
        "status": "ok" if not discrepancies else "blocked",
        "discrepancies": discrepancies,
    }


def preflight_host_state() -> tuple[bool, bytes, bytes]:
    bridge_ok = check_bridge()
    df = run(["df", "-B1", str(LIBVIRT_IMAGES)], check=False)
    rhel810 = run(
        ["sudo", "-n", "virsh", "--connect", "qemu:///system", "domstate", "rhel810"],
        check=False,
    )
    return bridge_ok, df.stdout, rhel810.stdout


def preflight_status(missing: list[str], bridge_ok: bool, rows: list[dict[str, Any]]) -> str:
    return "ok" if not missing and bridge_ok and all(row["status"] == "ok" for row in rows) else "blocked"


def preflight(targets: list[Target]) -> dict[str, Any]:
    ensure_local_dirs()
    tools = preflight_tools()
    missing = missing_preflight_tools(tools)
    rows = [preflight_target_row(target) for target in targets]
    bridge_ok, df_stdout, rhel810_stdout = preflight_host_state()
    report = {
        "schema": SCHEMA,
        "kind": "preflight",
        "generated_at_unix": int(time.time()),
        "caps": {"max_new_vms": 4, "name_prefix": "sdjournal-", "vcpus": 1, "memory_mib": 1024, "disk_gib": 4},
        "tools": {key: bool(value) for key, value in tools.items()},
        "bridge_br0": "ok" if bridge_ok else "missing",
        "libvirt_images_df_sha256": hashlib.sha256(df_stdout).hexdigest(),
        "rhel810_read_only_state": "running" if rhel810_stdout.strip() == b"running" else "unavailable",
        "targets": rows,
        "status": preflight_status(missing, bridge_ok, rows),
        "discrepancies": (["MISSING_TOOL"] if missing else []) + ([] if bridge_ok else ["BRIDGE_MISSING"]),
    }
    (STATE_DIR / "preflight.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def read_public_keys() -> list[str]:
    keys = []
    for name in ("id_ed25519.pub", "id_rsa.pub"):
        path = Path.home() / ".ssh" / name
        if path.exists():
            value = path.read_text().strip()
            if value:
                keys.append(value)
    if not keys:
        raise RuntimeError("no SSH public keys found in ~/.ssh")
    return keys


def image_cache_path(target: Target) -> Path:
    filename = Path(urlparse(target.image_url).path).name
    return IMAGE_CACHE / filename


def checksum_file(path: Path, algorithm: str) -> str:
    h = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def download_image(target: Target) -> Path:
    path = image_cache_path(target)
    if not path.exists() or checksum_file(path, target.checksum_algorithm) != target.checksum:
        run(["curl", "-fL", "--retry", "3", "-o", str(path), target.image_url], timeout=1800)
    actual = checksum_file(path, target.checksum_algorithm)
    if actual != target.checksum:
        raise RuntimeError(f"checksum mismatch for {target.alias}")
    return path


def build_seed_iso(target: Target) -> Path:
    keys = read_public_keys()
    work = SEED_WORK / target.name
    work.mkdir(parents=True, exist_ok=True)
    key_block = "\n".join(f"      - {key}" for key in keys)
    user_data = f"""#cloud-config
hostname: {target.name}
manage_etc_hosts: true
preserve_hostname: false
timezone: Europe/Athens
locale: en_US.UTF-8

users:
  - name: user
    groups: [sudo]
    shell: /bin/bash
    sudo: "ALL=(ALL) NOPASSWD:ALL"
    lock_passwd: true
    ssh_authorized_keys:
{key_block}

package_update: false
package_upgrade: false
package_reboot_if_required: false

runcmd:
  - [ sh, -c, "systemctl enable --now qemu-guest-agent || true" ]

final_message: "{target.name} cloud-init done after $UPTIME seconds"
"""
    meta_data = f"instance-id: {target.name}\nlocal-hostname: {target.name}\n"
    (work / "user-data").write_text(user_data)
    (work / "meta-data").write_text(meta_data)
    iso_tmp = SEED_WORK / f"{target.name}-seed.iso"
    run(
        [
            "genisoimage",
            "-output",
            str(iso_tmp),
            "-volid",
            "cidata",
            "-rock",
            "-joliet",
            str(work / "user-data"),
            str(work / "meta-data"),
        ],
        timeout=60,
    )
    iso_target = LIBVIRT_IMAGES / f"{target.name}-seed.iso"
    run(["sudo", "-n", "install", "-m", "0644", str(iso_tmp), str(iso_target)], timeout=60)
    return iso_target


def create_disk(target: Target, image: Path) -> Path:
    info = json.loads(run(["qemu-img", "info", "--output=json", str(image)], timeout=60).stdout.decode())
    if int(info["virtual-size"]) > DISK_CAP_BYTES:
        raise RuntimeError("IMAGE_TOO_LARGE_FOR_CAP")
    disk = LIBVIRT_IMAGES / f"{target.name}.qcow2"
    run(["sudo", "-n", "qemu-img", "convert", "-O", "qcow2", str(image), str(disk)], timeout=1800)
    run(["sudo", "-n", "qemu-img", "resize", str(disk), "4G"], timeout=120)
    run(["sudo", "-n", "chmod", "0660", str(disk)], timeout=30)
    run(
        [
            "sudo",
            "-n",
            "sh",
            "-c",
            f"getent passwd libvirt-qemu >/dev/null && chown libvirt-qemu:libvirt-qemu {str(disk)!r} || true",
        ],
        timeout=30,
    )
    return disk


def provision_one(target: Target) -> dict[str, Any]:
    if domain_exists(target.name):
        ip = wait_for_ip(target)
        wait_for_ssh(target, ip)
        state = {"alias": target.alias, "vm_name": target.name, "ip_b64": base64.b64encode(ip.encode()).decode()}
        (STATE_DIR / f"{target.alias}.json").write_text(json.dumps(state, indent=2) + "\n")
        return {
            "alias": target.alias,
            "status": "ok",
            "adopted_existing_sdjournal_domain": True,
            "resources": {"vcpus": 1, "memory_mib": 1024, "disk_gib": 4},
            "artifacts": {"domain": target.name, "disk": f"{target.name}.qcow2", "seed_iso": f"{target.name}-seed.iso"},
        }
    info = qemu_image_info(target.image_url)
    if int(info["virtual-size"]) > DISK_CAP_BYTES:
        return {"alias": target.alias, "status": "blocked", "discrepancies": ["IMAGE_TOO_LARGE_FOR_CAP"]}
    image = download_image(target)
    seed = build_seed_iso(target)
    disk = create_disk(target, image)
    cmd = [
        "sudo",
        "-n",
        "virt-install",
        "--connect",
        "qemu:///system",
        "--name",
        target.name,
        "--memory",
        "1024",
        "--vcpus",
        "1",
        "--cpu",
        "host-passthrough",
        "--osinfo",
        target.osinfo,
        "--import",
        "--disk",
        f"path={disk},format=qcow2,bus=virtio",
        "--disk",
        f"path={seed},device=cdrom,bus=sata,readonly=on",
        "--network",
        "bridge=br0,model=virtio",
        "--graphics",
        "none",
        "--console",
        "pty,target_type=serial",
        "--noautoconsole",
    ]
    run(cmd, timeout=300)
    ip = wait_for_ip(target)
    wait_for_ssh(target, ip)
    state = {"alias": target.alias, "vm_name": target.name, "ip_b64": base64.b64encode(ip.encode()).decode()}
    (STATE_DIR / f"{target.alias}.json").write_text(json.dumps(state, indent=2) + "\n")
    return {
        "alias": target.alias,
        "status": "ok",
        "resources": {"vcpus": 1, "memory_mib": 1024, "disk_gib": 4},
        "artifacts": {"domain": target.name, "disk": disk.name, "seed_iso": seed.name},
    }


def dom_mac(target: Target) -> str:
    result = run(["sudo", "-n", "virsh", "--connect", "qemu:///system", "domiflist", target.name], timeout=30)
    for line in result.stdout.decode().splitlines():
        parts = line.split()
        if len(parts) >= 5 and parts[1] == "bridge":
            return parts[4].lower()
    raise RuntimeError(f"no bridge interface found for {target.name}")


def scoped_link_local_ip(ip: str) -> str:
    return ip + "%br0" if ":" in ip and ip.startswith("fe80:") else ip


def agent_ipv4(stdout: bytes) -> str | None:
    for line in stdout.decode().splitlines():
        fields = line.split()
        if len(fields) >= 4 and fields[2] == "ipv4":
            ip = fields[3].split("/", 1)[0]
            if ip != "127.0.0.1":
                return ip
    return None


def neigh_line_mac(fields: list[str]) -> str | None:
    if "lladdr" in fields:
        mac_index = fields.index("lladdr") + 1
        return fields[mac_index].lower() if mac_index < len(fields) else None
    if len(fields) >= 5:
        return fields[4].lower()
    return None


def neigh_ip_for_mac(stdout: bytes, mac: str) -> str | None:
    for line in stdout.decode().splitlines():
        fields = line.split()
        if fields and neigh_line_mac(fields) == mac:
            return scoped_link_local_ip(fields[0])
    return None


def agent_ip_for_target(target: Target) -> str | None:
    agent = run(
        ["sudo", "-n", "virsh", "--connect", "qemu:///system", "domifaddr", target.name, "--source", "agent"],
        check=False,
        timeout=20,
    )
    return agent_ipv4(agent.stdout)


def neighbor_ip_for_mac(mac: str) -> str | None:
    neigh = run(["ip", "neigh", "show", "dev", "br0"], check=False, timeout=20)
    return neigh_ip_for_mac(neigh.stdout, mac)


def wait_for_ip(target: Target, timeout: int = 240) -> str:
    mac = dom_mac(target)
    deadline = time.time() + timeout
    while time.time() < deadline:
        ip = agent_ip_for_target(target) or neighbor_ip_for_mac(mac)
        if ip:
            return ip
        time.sleep(5)
    raise RuntimeError(f"timed out waiting for IP for {target.name}")


def ssh_base(ip: str) -> list[str]:
    ensure_local_dirs()
    cmd = [
        "ssh",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        f"UserKnownHostsFile={KNOWN_HOSTS}",
        "-o",
        "GlobalKnownHostsFile=/dev/null",
        f"user@{ip}",
    ]
    if ":" in ip:
        cmd.insert(1, "-6")
    return cmd


def scp_base() -> list[str]:
    ensure_local_dirs()
    cmd = [
        "scp",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        f"UserKnownHostsFile={KNOWN_HOSTS}",
        "-o",
        "GlobalKnownHostsFile=/dev/null",
    ]
    return cmd


def scp_remote(ip: str, remote_path: str) -> str:
    if ":" in ip:
        return f"user@[{ip}]:{remote_path}"
    return f"user@{ip}:{remote_path}"


def wait_for_ssh(target: Target, ip: str, timeout: int = 300) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        result = run(ssh_base(ip) + ["true"], check=False, timeout=15)
        if result.returncode == 0:
            cloud = run(ssh_base(ip) + ["sudo", "-n", "cloud-init", "status", "--wait"], check=False, timeout=240)
            if cloud.returncode in (0, 2):
                return
        time.sleep(5)
    raise RuntimeError(f"timed out waiting for SSH/cloud-init for {target.name}")


REMOTE_GENERATE = r"""#!/usr/bin/env bash
set -euo pipefail
phase="${1:?phase required}"
out=/home/user/sdjournal-out
sudo install -d -o user -g user "$out"
sudo mkdir -p /var/log/journal /etc/systemd/journald.conf.d

write_config() {
  local compress="$1"
  sudo tee /etc/systemd/journald.conf.d/99-sdjournal-sdk.conf >/dev/null <<EOF
[Journal]
Storage=persistent
Compress=${compress}
SystemMaxUse=128M
RuntimeMaxUse=64M
MaxRetentionSec=1month
EOF
  sudo systemctl restart systemd-journald
  sudo journalctl --flush >/dev/null 2>&1 || true
}

emit_case() {
  local case_id="$1"
  local large
  large="$(printf 'sdjournal-large-%04096d' 7)"
  for i in $(seq 1 20); do
    printf 'SOW0075_CASE=%s entry=%03d payload=%s\n' "$case_id" "$i" "$large" |
      systemd-cat -t sdjournal-sow0075 -p info
  done
}

find_active() {
  sudo find /var/log/journal /run/log/journal -type f -name '*.journal' ! -name '*@*.journal' 2>/dev/null | sort | tail -1
}

find_archived() {
  sudo find /var/log/journal /run/log/journal -type f -name '*@*.journal' 2>/dev/null | sort | tail -1
}

copy_case() {
  local source="$1"
  local case_id="$2"
  if [ -n "$source" ] && [ -f "$source" ]; then
    sudo cp "$source" "$out/${case_id}.journal"
    sudo chown user:user "$out/${case_id}.journal"
  fi
}

verify_cases() {
  : > "$out/vm-verify.jsonl"
  for file in "$out"/*.journal; do
    [ -e "$file" ] || continue
    case_id="$(basename "$file" .journal)"
    stdout="$(mktemp)"
    stderr="$(mktemp)"
    rc=0
    sudo journalctl --verify --file "$file" >"$stdout" 2>"$stderr" || rc=$?
    out_sha="$(sha256sum "$stdout" | awk '{print $1}')"
    err_sha="$(sha256sum "$stderr" | awk '{print $1}')"
    printf '{"case_id":"%s","returncode":%s,"stdout_sha256":"%s","stderr_sha256":"%s"}\n' \
      "$case_id" "$rc" "$out_sha" "$err_sha" >> "$out/vm-verify.jsonl"
    rm -f "$stdout" "$stderr"
  done
}

if [ "$phase" = "initial" ]; then
  rm -f "$out"/*.journal "$out"/*.json "$out"/*.jsonl "$out"/*.txt 2>/dev/null || true
  write_config yes
  emit_case compress-on
  copy_case "$(find_active)" compress-on-active
  sudo journalctl --rotate >/dev/null 2>&1 || true
  sleep 1
  copy_case "$(find_archived)" compress-on-archived
  write_config no
  emit_case compress-off
  copy_case "$(find_active)" compress-off-active
  sudo journalctl --rotate >/dev/null 2>&1 || true
  sleep 1
  copy_case "$(find_archived)" compress-off-archived
  if logger --help 2>&1 | grep -q -- '--journald'; then
    printf 'MESSAGE=sow0075 logger journald field path\nSOW0075_LOGGER_FIELD=text\n' | logger --journald || true
    echo "logger-journald-text-fields" > "$out/binary-field-status.txt"
  else
    echo "unsupported-by-stock-tooling" > "$out/binary-field-status.txt"
  fi
elif [ "$phase" = "post-reboot" ]; then
  emit_case post-reboot
  copy_case "$(find_active)" post-reboot-active
  sudo journalctl --rotate >/dev/null 2>&1 || true
  sleep 1
  copy_case "$(find_archived)" post-reboot-archived
else
  echo "unknown phase: $phase" >&2
  exit 2
fi

journalctl --version | head -1 > "$out/systemd-version.txt"
. /etc/os-release
printf '%s\n' "${ID:-unknown} ${VERSION_ID:-unknown}" > "$out/os-release.txt"
verify_cases
"""


def load_ip(target: Target) -> str:
    state = json.loads((STATE_DIR / f"{target.alias}.json").read_text())
    return base64.b64decode(state["ip_b64"]).decode()


def ssh_run(ip: str, cmd: list[str], *, input_bytes: bytes | None = None, timeout: int = 300, check: bool = True):
    return run(ssh_base(ip) + cmd, input_bytes=input_bytes, timeout=timeout, check=check)


def collect_one(target: Target) -> dict[str, Any]:
    ip = load_ip(target)
    ssh_run(ip, ["sudo", "-n", "bash", "-s", "--", "initial"], input_bytes=REMOTE_GENERATE.encode(), timeout=300)
    ssh_run(ip, ["sudo", "-n", "reboot"], check=False, timeout=10)
    time.sleep(10)
    ip = wait_for_ip(target)
    wait_for_ssh(target, ip)
    (STATE_DIR / f"{target.alias}.json").write_text(
        json.dumps({"alias": target.alias, "vm_name": target.name, "ip_b64": base64.b64encode(ip.encode()).decode()}, indent=2)
        + "\n"
    )
    ssh_run(ip, ["sudo", "-n", "bash", "-s", "--", "post-reboot"], input_bytes=REMOTE_GENERATE.encode(), timeout=300)
    local_raw = RAW_DIR / target.alias
    if local_raw.exists():
        shutil.rmtree(local_raw)
    local_raw.mkdir(parents=True)
    run(scp_base() + [scp_remote(ip, "/home/user/sdjournal-out/*"), str(local_raw) + "/"], timeout=600)
    journal_count = len(list(local_raw.glob("*.journal")))
    return {"alias": target.alias, "status": "ok", "journal_files": journal_count}


def build_digest_helpers() -> dict[str, Path | None]:
    ensure_local_dirs()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(LOCAL / "cargo-target")
    env["CARGO_HOME"] = str(LOCAL / "cargo-home")
    env["GOCACHE"] = str(LOCAL / "go-cache")
    env["GOMODCACHE"] = str(LOCAL / "go-mod")
    env["GOPATH"] = str(LOCAL / "go-path")
    rust = None
    go = None
    if which("cargo"):
        run(["cargo", "build", "--manifest-path", str(ROOT / "rust" / "Cargo.toml"), "--release", "-p", "corpus_digest"], env=env, timeout=1800)
        rust = Path(env["CARGO_TARGET_DIR"]) / "release" / "corpus_digest"
    if which("go"):
        go = BIN_DIR / "go-corpus-digest"
        run(["go", "build", "-o", str(go), "./internal/testcmd/corpus_digest"], cwd=ROOT / "go", env=env, timeout=1800)
    return {"rust": rust, "go": go}


def digest_stock(path: Path) -> tuple[dict[str, Any] | None, dict[str, Any]]:
    cmd = ["journalctl", "--file", str(path), "--output=export", "--all", "--no-pager", "--quiet"]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(cmd, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE)  # nosec B603
    assert proc.stdout is not None
    started = time.perf_counter()
    try:
        digest = digest_export_stream(proc.stdout)
    except Exception as exc:
        proc.kill()
        stderr = proc.stderr.read() if proc.stderr else b""
        return None, {
            "status": "failed",
            "returncode": None,
            "command_sha256": command_sha256(cmd),
            "stderr_sha256": hashlib.sha256(stderr + str(exc).encode()).hexdigest(),
        }
    stderr = proc.stderr.read() if proc.stderr else b""
    rc = proc.wait(timeout=60)
    stats = {
        "status": "ok" if rc == 0 else "failed",
        "returncode": rc,
        "command_sha256": command_sha256(cmd),
        "stderr_sha256": hashlib.sha256(stderr).hexdigest(),
        "elapsed_seconds": round(time.perf_counter() - started, 6),
    }
    if rc != 0:
        return None, stats
    digest["driver"] = "stock"
    return digest, stats


def run_json_digest(driver: str, exe: Path, path: Path) -> tuple[dict[str, Any] | None, dict[str, Any]]:
    cmd = [str(exe), "--input", str(path), "--bounds", "snapshot"]
    started = time.perf_counter()
    result = run(cmd, check=False, timeout=180)
    stats = {
        "status": "ok" if result.returncode == 0 else "failed",
        "returncode": result.returncode,
        "command_sha256": command_sha256(cmd),
        "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
        "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
        "elapsed_seconds": round(time.perf_counter() - started, 6),
    }
    if result.returncode != 0:
        return None, stats
    payload = json.loads(result.stdout.decode())
    payload["driver"] = driver
    return payload, stats


def digest_export_command(driver: str, cmd: list[str]) -> tuple[dict[str, Any] | None, dict[str, Any]]:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.Popen(  # nosec B603
        cmd,  # nosemgrep
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdout is not None
    started = time.perf_counter()
    try:
        digest = digest_export_stream(proc.stdout)
    except Exception as exc:
        proc.kill()
        stderr = proc.stderr.read() if proc.stderr else b""
        return None, {
            "status": "failed",
            "returncode": None,
            "command_sha256": command_sha256(cmd),
            "stderr_sha256": hashlib.sha256(stderr + str(exc).encode()).hexdigest(),
        }
    stderr = proc.stderr.read() if proc.stderr else b""
    rc = proc.wait(timeout=180)
    stats = {
        "status": "ok" if rc == 0 else "failed",
        "returncode": rc,
        "command_sha256": command_sha256(cmd),
        "stderr_sha256": hashlib.sha256(stderr).hexdigest(),
        "elapsed_seconds": round(time.perf_counter() - started, 6),
    }
    if rc != 0:
        return None, stats
    digest["driver"] = driver
    return digest, stats


def verify_file(path: Path) -> dict[str, Any]:
    cmd = ["journalctl", "--verify", "--file", str(path)]
    result = run(cmd, check=False, timeout=180)
    return {
        "status": "ok" if result.returncode == 0 else "failed",
        "returncode": result.returncode,
        "command_sha256": command_sha256(cmd),
        "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
        "stderr_sha256": hashlib.sha256(result.stderr).hexdigest(),
    }


def read_text_default(path: Path, default: str = "") -> str:
    return path.read_text().strip() if path.exists() else default


def read_vm_verify(raw: Path) -> dict[str, Any]:
    verify_path = raw / "vm-verify.jsonl"
    rows: dict[str, Any] = {}
    if not verify_path.exists():
        return rows
    for line in verify_path.read_text().splitlines():
        if line.strip():
            row = json.loads(line)
            rows[row["case_id"]] = row
    return rows


def add_compiled_reader(
    readers: dict[str, Any],
    discrepancies: list[str],
    helpers: dict[str, Path | None],
    driver: str,
    path: Path,
) -> None:
    exe = helpers.get(driver)
    if exe is None:
        readers[driver] = {"digest": None, "stats": {"status": "skipped", "reason": "tool unavailable"}}
        discrepancies.append(f"{driver.upper()}_READ_FAILED")
        return
    digest, stats = run_json_digest(driver, exe, path)
    readers[driver] = {"digest": digest, "stats": stats}


def add_python_reader(readers: dict[str, Any], path: Path) -> None:
    py = os.environ.get("SOW0075_PYTHON") or which("python3")
    if not py:
        return
    digest, stats = digest_export_command(
        "python",
        [py, str(ROOT / "python" / "cmd" / "journalctl.py"), "--file", str(path), "--output=export"],
    )
    readers["python"] = {"digest": digest, "stats": stats}


def add_node_reader(readers: dict[str, Any], path: Path) -> None:
    node = which("node")
    if not node:
        return
    digest, stats = digest_export_command(
        "node",
        [node, str(ROOT / "node" / "cmd" / "journalctl" / "index.js"), "--file", str(path), "--output", "export"],
    )
    readers["node"] = {"digest": digest, "stats": stats}


def compare_case_readers(readers: dict[str, Any], baseline: str | None) -> list[str]:
    discrepancies: list[str] = []
    for driver, row in readers.items():
        if driver == "stock":
            continue
        digest = row["digest"]
        if digest is None:
            discrepancies.append(f"{driver.upper()}_READ_FAILED")
        elif baseline and digest.get("logical_digest") != baseline:
            discrepancies.append(f"{driver.upper()}_DIGEST_MISMATCH")
    return discrepancies


def validate_case(path: Path, helpers: dict[str, Path | None], vm_verify: dict[str, Any]) -> dict[str, Any]:
    case_id = path.stem
    host_verify = verify_file(path)
    stock_digest, stock_stats = digest_stock(path)
    readers = {"stock": {"digest": stock_digest, "stats": stock_stats}}
    discrepancies: list[str] = []

    if host_verify["status"] != "ok":
        discrepancies.append("HOST_STOCK_VERIFY_FAILED")
    if stock_digest is None:
        discrepancies.append("STOCK_READ_FAILED")
    for driver in ("rust", "go"):
        add_compiled_reader(readers, discrepancies, helpers, driver, path)
    add_python_reader(readers, path)
    add_node_reader(readers, path)
    discrepancies.extend(compare_case_readers(readers, stock_digest.get("logical_digest") if stock_digest else None))
    vm_row = vm_verify.get(case_id)
    if vm_row and int(vm_row.get("returncode", 1)) != 0:
        discrepancies.append("VM_STOCK_VERIFY_FAILED")
    return {
        "case_id": case_id,
        "file": {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()},
        "vm_stock_verify": vm_row,
        "host_stock_verify": host_verify,
        "readers": readers,
        "status": "ok" if not discrepancies else "discrepancy",
        "discrepancies": sorted(set(discrepancies)),
    }


def validate_one(target: Target, helpers: dict[str, Path | None]) -> dict[str, Any]:
    raw = RAW_DIR / target.alias
    vm_verify = read_vm_verify(raw)
    cases = [validate_case(path, helpers, vm_verify) for path in sorted(raw.glob("*.journal"))]
    return {
        "alias": target.alias,
        "vm_name": target.name,
        "distro": target.distro,
        "expected_systemd": target.expected_systemd,
        "observed_systemd": read_text_default(raw / "systemd-version.txt"),
        "observed_os_release": read_text_default(raw / "os-release.txt"),
        "binary_field_ingestion": read_text_default(raw / "binary-field-status.txt", "unknown"),
        "cases": cases,
        "status": "ok" if cases and all(case["status"] == "ok" for case in cases) else "discrepancy",
    }


def validate(targets: list[Target], report_json: Path, report_md: Path) -> dict[str, Any]:
    helpers = build_digest_helpers()
    target_reports = [validate_one(target, helpers) for target in targets]
    discrepancies = sorted(
        {
            code
            for target in target_reports
            for case in target["cases"]
            for code in case["discrepancies"]
        }
    )
    report = {
        "schema": SCHEMA,
        "kind": "vm-reader-matrix",
        "generated_at_unix": int(time.time()),
        "canonical_digest_schema": SCHEMA_VERSION,
        "python_runtime": os.environ.get("SOW0075_PYTHON_LABEL", "python3"),
        "caps": {"max_new_vms": 4, "name_prefix": "sdjournal-", "vcpus": 1, "memory_mib": 1024, "disk_gib": 4},
        "targets": target_reports,
        "status": "ok" if not discrepancies and all(t["status"] == "ok" for t in target_reports) else "discrepancy",
        "discrepancies": discrepancies,
    }
    report_json.parent.mkdir(parents=True, exist_ok=True)
    report_json.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    report_md.write_text(markdown_report(report))
    return report


def append_markdown_header(lines: list[str], report: dict[str, Any]) -> None:
    lines.extend([
        "# SOW-0075 VM Historical systemd Matrix Report",
        "",
        f"- Schema: `{report['schema']}`",
        f"- Status: `{report['status']}`",
        f"- Canonical digest schema: `{report['canonical_digest_schema']}`",
        f"- Python runtime: `{report.get('python_runtime', 'python3')}`",
        f"- Discrepancies: `{', '.join(report['discrepancies']) if report['discrepancies'] else 'none'}`",
        "",
        "| target | observed systemd | cases | status | discrepancy codes |",
        "|---|---:|---:|---|---|",
    ])


def target_discrepancy_codes(target: dict[str, Any]) -> str:
    codes = sorted({code for case in target["cases"] for code in case["discrepancies"]})
    return ", ".join(codes) if codes else "none"


def append_target_summary(lines: list[str], report: dict[str, Any]) -> None:
    for target in report["targets"]:
        lines.append(
            "| {alias} | `{systemd}` | {cases} | `{status}` | `{codes}` |".format(
                alias=target["alias"],
                systemd=target["observed_systemd"] or "unknown",
                cases=len(target["cases"]),
                status=target["status"],
                codes=target_discrepancy_codes(target),
            )
        )


def case_reader_parity(case: dict[str, Any]) -> str:
    baseline = case["readers"]["stock"]["digest"]
    if baseline is None:
        return "failed"
    digest = baseline.get("logical_digest")
    for driver, row in case["readers"].items():
        if driver != "stock" and (row["digest"] is None or row["digest"].get("logical_digest") != digest):
            return "failed"
    return "ok"


def append_case_table(lines: list[str], target: dict[str, Any]) -> None:
    lines.append(f"### {target['alias']}")
    lines.append("")
    lines.append(f"- Distro: `{target['distro']}`")
    lines.append(f"- OS release: `{target['observed_os_release'] or 'unknown'}`")
    lines.append(f"- Binary field ingestion: `{target['binary_field_ingestion']}`")
    lines.append("")
    lines.append("| case | bytes | stock verify | reader parity | status |")
    lines.append("|---|---:|---|---|---|")
    for case in target["cases"]:
        lines.append(
            f"| `{case['case_id']}` | {case['file']['bytes']} | "
            f"`{case['host_stock_verify']['status']}` | `{case_reader_parity(case)}` | "
            f"`{case['status']}` |"
        )
    lines.append("")


def append_case_results(lines: list[str], report: dict[str, Any]) -> None:
    lines.extend(["", "## Case Results", ""])
    for target in report["targets"]:
        append_case_table(lines, target)


def markdown_report(report: dict[str, Any]) -> str:
    lines: list[str] = []
    append_markdown_header(lines, report)
    append_target_summary(lines, report)
    append_case_results(lines, report)
    return "\n".join(lines) + "\n"


def cmd_preflight(args: argparse.Namespace) -> None:
    report = preflight(selected_targets(args.targets))
    print(json.dumps(report, indent=2, sort_keys=True))


def cmd_provision(args: argparse.Namespace) -> None:
    rows = [provision_one(target) for target in selected_targets(args.targets)]
    path = STATE_DIR / "provision.json"
    path.write_text(json.dumps({"schema": SCHEMA, "kind": "provision", "results": rows}, indent=2, sort_keys=True) + "\n")
    print(json.dumps(rows, indent=2, sort_keys=True))


def cmd_collect(args: argparse.Namespace) -> None:
    rows = [collect_one(target) for target in selected_targets(args.targets)]
    path = STATE_DIR / "collect.json"
    path.write_text(json.dumps({"schema": SCHEMA, "kind": "collect", "results": rows}, indent=2, sort_keys=True) + "\n")
    print(json.dumps(rows, indent=2, sort_keys=True))


def cmd_validate(args: argparse.Namespace) -> None:
    report = validate(selected_targets(args.targets), args.report_json, args.report_md)
    print(json.dumps({"status": report["status"], "discrepancies": report["discrepancies"]}, indent=2))


def cmd_run(args: argparse.Namespace) -> None:
    targets = selected_targets(args.targets)
    pf = preflight(targets)
    if pf["status"] != "ok":
        raise SystemExit("preflight blocked; see .local/sow-0075/state/preflight.json")
    rows = [provision_one(target) for target in targets]
    blocked = [row for row in rows if row["status"] != "ok"]
    if blocked:
        raise SystemExit("provisioning blocked; see .local/sow-0075/state/provision.json")
    (STATE_DIR / "provision.json").write_text(json.dumps({"schema": SCHEMA, "kind": "provision", "results": rows}, indent=2) + "\n")
    collect_rows = [collect_one(target) for target in targets]
    (STATE_DIR / "collect.json").write_text(json.dumps({"schema": SCHEMA, "kind": "collect", "results": collect_rows}, indent=2) + "\n")
    report = validate(targets, args.report_json, args.report_md)
    print(json.dumps({"status": report["status"], "discrepancies": report["discrepancies"]}, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    for name, func in [
        ("preflight", cmd_preflight),
        ("provision", cmd_provision),
        ("collect", cmd_collect),
        ("validate", cmd_validate),
        ("run", cmd_run),
    ]:
        p = sub.add_parser(name)
        p.add_argument("--targets", nargs="+", default=["all"], help="target aliases or 'all'")
        p.add_argument("--report-json", type=Path, default=ROOT / "tests" / "vm_matrix" / "reports" / "sow-0075-vm-matrix-report.json")
        p.add_argument("--report-md", type=Path, default=ROOT / "tests" / "vm_matrix" / "reports" / "sow-0075-vm-matrix-report.md")
        p.set_defaults(func=func)
    args = parser.parse_args()
    args.func(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
