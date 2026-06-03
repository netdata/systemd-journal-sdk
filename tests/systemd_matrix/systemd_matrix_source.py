"""systemd source/build patch helpers for the version matrix harness."""

from __future__ import annotations

import shutil
import subprocess  # nosec B404
from pathlib import Path
from typing import Any

from tests.systemd_matrix.systemd_matrix_runtime import (
    ROOT,
    matrix_env,
    run_capture,
    sha256_file,
    version_slug,
)


SYSTEMD_HELPER_SOURCE = (
    ROOT / "tests" / "datasets" / "ingesters" / "systemd" / "dataset_ingester.c"
)
SYSTEMD_HELPER_NAME = "test-systemd-matrix-ingester"
SYSTEMD_HELPER_SOURCE_NAME = f"{SYSTEMD_HELPER_NAME}.c"


def maybe_meson_option(options_text: str, name: str, value: str) -> list[str]:
    if f"option('{name}'" in options_text or f'option("{name}"' in options_text:
        if value == "disabled" and f"option('{name}', type : 'combo', choices : ['auto', 'true', 'false']" in options_text:
            value = "false"
        return [f"-D{name}={value}"]
    return []


def resolve_systemd_ref(systemd_src: Path, version: str, explicit_ref: str | None) -> str:
    ref = explicit_ref or version
    cmd = ["git", "-C", str(systemd_src), "rev-parse", f"{ref}^{{commit}}"]
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.run(cmd, cwd=str(ROOT), text=True, capture_output=True, check=False)  # nosec B603
    if proc.returncode != 0:
        raise RuntimeError(f"could not resolve systemd ref {ref!r} from {systemd_src}")
    return proc.stdout.strip()


def is_git_checkout(path: Path) -> bool:
    # nosemgrep
    # subprocess is required by this harness; commands are shell=False vectors.
    proc = subprocess.run(  # nosec B603
        ["git", "-C", str(path), "rev-parse", "--show-toplevel"],
        cwd=str(ROOT),
        text=True,
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        return False
    return Path(proc.stdout.strip()).resolve() == path.resolve()


def non_git_source_fingerprint(path: Path) -> str:
    """Return a report-only identifier for unpacked release source trees."""
    import hashlib

    digest = hashlib.sha256()
    for relative_name in (
        "meson.build",
        "meson_options.txt",
        "NEWS",
        "src/libsystemd/sd-journal/journal-file.c",
        "src/libsystemd/sd-journal/journal-authenticate.c",
    ):
        item = path / relative_name
        if not item.exists():
            continue
        digest.update(relative_name.encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha256_file(item).encode("ascii"))
        digest.update(b"\0")
    return f"non-git-source-sha256:{digest.hexdigest()[:24]}"


def ensure_systemd_source(
    version: str,
    *,
    out: Path,
    systemd_src: Path,
    source_ref: str | None,
    timeout: int,
) -> tuple[Path, str, list[dict[str, Any]]]:
    slug = version_slug(version)
    version_root = out / "builds" / slug
    source_dir = version_root / "source"
    commands: list[dict[str, Any]] = []
    if not is_git_checkout(systemd_src):
        if not (systemd_src / "meson.build").exists():
            raise RuntimeError(f"systemd source tree is not buildable: {systemd_src}")
        if source_dir.exists():
            shutil.rmtree(source_dir)
        source_dir.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(
            systemd_src,
            source_dir,
            symlinks=True,
            ignore=shutil.ignore_patterns(".git", "build", "__pycache__"),
        )
        return source_dir, non_git_source_fingerprint(systemd_src), commands

    commit = resolve_systemd_ref(systemd_src, version, source_ref)
    if not (source_dir / ".git").exists():
        source_dir.parent.mkdir(parents=True, exist_ok=True)
        cmd = ["git", "clone", "--no-checkout", "--local", str(systemd_src), str(source_dir)]
        result, _ = run_capture(
            "clone systemd source",
            cmd,
            env=matrix_env(out),
            timeout=timeout,
        )
        commands.append(result.as_dict())
        if result.returncode != 0:
            raise RuntimeError("systemd source clone failed")
    for label, cmd in (
        (
            "fetch requested systemd commit",
            ["git", "-C", str(source_dir), "fetch", "--tags", str(systemd_src), commit],
        ),
        (
            "checkout requested systemd commit",
            ["git", "-C", str(source_dir), "checkout", "--detach", commit],
        ),
    ):
        result, _ = run_capture(label, cmd, env=matrix_env(out), timeout=timeout)
        commands.append(result.as_dict())
        if result.returncode != 0:
            raise RuntimeError(f"{label} failed")
    return source_dir, commit, commands


def meson_helper_insertion(text: str) -> str:
    marker = """        {
                'sources' : files('sd-journal/test-journal-append.c'),
                'type' : 'manual',
        },
"""
    entry = f"""        {{
                'sources' : files('sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}'),
                'type' : 'manual',
        }},
"""
    if marker in text:
        return text.replace(marker, marker + entry)

    simple_marker = "        'sd-journal/test-journal-file.c',\n"
    simple_entry = f"        'sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}',\n"
    if simple_marker in text:
        return text.replace(simple_marker, simple_marker + simple_entry)

    legacy_marker = """        [files('sd-journal/test-format-change-ingester.c'),
         [], [], [], '', 'manual'],
"""
    legacy_entry = f"""        [files('sd-journal/{SYSTEMD_HELPER_SOURCE_NAME}'),
         [], [], [], '', 'manual'],
"""
    if legacy_marker in text:
        return text.replace(legacy_marker, legacy_marker + legacy_entry)
    raise RuntimeError("could not find systemd meson journal test marker")


def patch_meson_helper(source_dir: Path) -> None:
    meson_file = source_dir / "src" / "libsystemd" / "meson.build"
    text = meson_file.read_text(encoding="utf-8")
    if SYSTEMD_HELPER_SOURCE_NAME in text:
        return
    meson_file.write_text(meson_helper_insertion(text), encoding="utf-8")


def fss_root_replacements() -> tuple[tuple[str, str], ...]:
    new_path = """        const char *fss_root = getenv("SYSTEMD_JOURNAL_FSS_ROOT");
        if (!fss_root || !*fss_root)
                fss_root = "/var/log/journal";

        if (asprintf(&path, "%s/" SD_ID128_FORMAT_STR "/fss",
                     fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    old_path = """        if (asprintf(&path, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
                     SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    new_legacy = """        const char *fss_root = getenv("SYSTEMD_JOURNAL_FSS_ROOT");
        if (!fss_root || !*fss_root)
                fss_root = "/var/log/journal";

        if (asprintf(&p, "%s/" SD_ID128_FORMAT_STR "/fss",
                     fss_root, SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    old_legacy = """        if (asprintf(&p, "/var/log/journal/" SD_ID128_FORMAT_STR "/fss",
                     SD_ID128_FORMAT_VAL(machine)) < 0)
                return -ENOMEM;
"""
    return ((old_path, new_path), (old_legacy, new_legacy))


def patch_authenticate_fss_root(source_dir: Path) -> None:
    authenticate_file = source_dir / "src" / "libsystemd" / "sd-journal" / "journal-authenticate.c"
    text = authenticate_file.read_text(encoding="utf-8")
    if "SYSTEMD_JOURNAL_FSS_ROOT" in text:
        return

    if "#include <stdlib.h>" not in text:
        text = text.replace("#include <unistd.h>\n", "#include <stdlib.h>\n#include <unistd.h>\n")
    for old, new in fss_root_replacements():
        if old in text:
            authenticate_file.write_text(text.replace(old, new), encoding="utf-8")
            return
    raise RuntimeError("could not find systemd journal FSS path marker")


def patch_filesystems_gperf(source_dir: Path) -> None:
    filesystems_file = source_dir / "src" / "basic" / "filesystems-gperf.gperf"
    if not filesystems_file.exists():
        return
    text = filesystems_file.read_text(encoding="utf-8")
    additions = {
        "bcachefs,": "bcachefs,        {BCACHEFS_SUPER_MAGIC}\n",
        "guest_memfd,": "guest_memfd,     {GUEST_MEMFD_MAGIC}\n",
        "pidfs,": "pidfs,           {PID_FS_MAGIC}\n",
    }
    missing = [line for marker, line in additions.items() if marker not in text]
    if missing:
        filesystems_file.write_text(text.rstrip() + "\n" + "".join(missing), encoding="utf-8")


def patch_systemd_helper(source_dir: Path) -> None:
    helper_dest = source_dir / "src" / "libsystemd" / "sd-journal" / SYSTEMD_HELPER_SOURCE_NAME
    shutil.copyfile(SYSTEMD_HELPER_SOURCE, helper_dest)
    patch_meson_helper(source_dir)
    patch_authenticate_fss_root(source_dir)
    patch_filesystems_gperf(source_dir)
