#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

from versioning import PLUGIN_CONFIGS, ensure_synced, read_cargo_version, set_version, validate_version


REPO_ROOT = Path(__file__).resolve().parents[1]
VERSION_FILES = [REPO_ROOT / "Cargo.toml", *PLUGIN_CONFIGS]


def run(cmd: list[str], *, capture: bool = False, check: bool = True) -> str:
    result = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        check=check,
        text=True,
        stdout=subprocess.PIPE if capture else None,
        stderr=subprocess.PIPE if capture else None,
    )
    return result.stdout.strip() if capture else ""


def ensure_clean_worktree() -> None:
    status = run(["git", "status", "--porcelain"], capture=True)
    if status:
        raise SystemExit("refusing to release from a dirty worktree")


def default_branch(remote: str) -> str:
    ref = run(["git", "symbolic-ref", "--quiet", f"refs/remotes/{remote}/HEAD"], capture=True, check=False)
    if ref:
        return ref.rsplit("/", 1)[-1]
    return "main"


def current_branch() -> str:
    branch = run(["git", "rev-parse", "--abbrev-ref", "HEAD"], capture=True)
    if branch == "HEAD":
        raise SystemExit("detached HEAD is not a sane release base")
    return branch


def tag_exists(tag: str) -> bool:
    return subprocess.run(["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"], cwd=REPO_ROOT).returncode == 0


def maybe_commit_version_bump(version: str) -> bool:
    files = [str(path.relative_to(REPO_ROOT)) for path in VERSION_FILES]
    run(["git", "add", *files])
    staged = subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=REPO_ROOT)
    if staged.returncode == 0:
        return False
    run(["git", "commit", "-m", f"chore(release): v{version}"])
    return True


def create_tag(tag: str) -> None:
    run(["git", "tag", "-a", tag, "-m", f"Release {tag}"])


def push_release(remote: str, branch: str, tag: str) -> None:
    run(["git", "push", remote, branch])
    run(["git", "push", remote, tag])


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create and push a tagged GitHub release.")
    parser.add_argument("--version", required=True, help="Release version, for example 0.2.3")
    parser.add_argument("--remote", default="origin")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--allow-non-default-branch", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    version = validate_version(args.version)
    tag = f"v{version}"

    ensure_clean_worktree()
    run(["git", "fetch", "--tags", args.remote])

    branch = current_branch()
    release_branch = default_branch(args.remote)
    if not args.allow_non_default_branch and branch != release_branch:
        raise SystemExit(
            f"refusing to release from {branch}; expected {release_branch}. "
            "Use --allow-non-default-branch if you really mean it."
        )

    if tag_exists(tag):
        raise SystemExit(f"tag {tag} already exists")

    current_version = ensure_synced()
    if args.dry_run:
        action = "retag current version" if current_version == version else "bump version files, commit, and tag"
        print(
            f"dry-run: would {action} on branch {branch}, then push {branch} and {tag} to {args.remote}"
        )
        return 0

    if current_version != version:
        set_version(version)
        ensure_synced(version)

    if not args.skip_tests:
        run(["cargo", "test"])

    maybe_commit_version_bump(version)
    create_tag(tag)
    push_release(args.remote, branch, tag)
    print(f"pushed {tag} from {branch}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
