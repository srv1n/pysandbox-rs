#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import textwrap
import urllib.error
import urllib.request
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MODEL = "gpt-5.4-mini"


def run(cmd: list[str]) -> str:
    return subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def previous_tag(current_tag: str) -> str | None:
    tags = run(["git", "tag", "--sort=version:refname"]).splitlines()
    tags = [tag.strip() for tag in tags if tag.strip()]
    if current_tag not in tags:
        return tags[-1] if tags else None
    index = tags.index(current_tag)
    if index == 0:
        return None
    return tags[index - 1]


def rev_range(prev_tag: str | None, current_tag: str) -> str:
    if prev_tag:
        return f"{prev_tag}..{current_tag}"
    return current_tag


def commit_lines(revision_range: str) -> list[str]:
    output = run(
        [
            "git",
            "log",
            "--first-parent",
            "--pretty=format:%h %s",
            revision_range,
        ]
    )
    return [line for line in output.splitlines() if line.strip()]


def changed_files(revision_range: str) -> list[str]:
    output = run(["git", "diff", "--name-only", revision_range])
    return [line for line in output.splitlines() if line.strip()]


def diff_stat(revision_range: str) -> str:
    return run(["git", "diff", "--shortstat", revision_range])


def top_paths(files: list[str], limit: int = 8) -> list[str]:
    counts: dict[str, int] = {}
    for path in files:
        parts = path.split("/")
        root = parts[0] if len(parts) == 1 else "/".join(parts[:2])
        counts[root] = counts.get(root, 0) + 1
    ordered = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    return [f"{path} ({count} files)" for path, count in ordered[:limit]]


def fallback_notes(
    *,
    repo: str,
    tag: str,
    version: str,
    prev_tag: str | None,
    commits: list[str],
    stat: str,
    files: list[str],
) -> str:
    heading = f"# rzn-python-sandbox {version}"
    scope = f"Changes since `{prev_tag}`." if prev_tag else "First tagged GitHub release."
    highlights = commits[:10] or ["Initial tagged release."]
    touched = top_paths(files)

    lines = [
        heading,
        "",
        scope,
        "",
        "## Highlights",
    ]
    lines.extend(f"- {line}" for line in highlights)
    lines.extend(
        [
            "",
            "## Install",
            f"- macOS/Linux: `curl -fsSL https://raw.githubusercontent.com/{repo}/main/scripts/install_rzn_python_tools.sh | sh -s -- --version {version} --github-repo {repo} --variant system`",
            f"- Windows: `powershell -ExecutionPolicy Bypass -File .\\scripts\\install_rzn_python_tools.ps1 -Version {version} -GitHubRepo {repo}`",
        ]
    )
    if stat:
        lines.extend(["", "## Scope", f"- {stat}"])
    if touched:
        lines.append("- Most-touched paths:")
        lines.extend(f"- {item}" for item in touched)
    if prev_tag:
        lines.extend(["", f"Full diff: https://github.com/{repo}/compare/{prev_tag}...{tag}"])
    else:
        lines.extend(["", f"Release page: https://github.com/{repo}/releases/tag/{tag}"])
    return "\n".join(lines) + "\n"


def extract_output_text(payload: object) -> str:
    texts: list[str] = []

    def walk(node: object) -> None:
        if isinstance(node, dict):
            node_type = node.get("type")
            if node_type in {"output_text", "text"} and isinstance(node.get("text"), str):
                texts.append(str(node["text"]))
            if isinstance(node.get("output_text"), str):
                texts.append(str(node["output_text"]))
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    walk(payload)
    return "\n".join(part.strip() for part in texts if part and part.strip()).strip()


def llm_notes(
    *,
    repo: str,
    tag: str,
    version: str,
    prev_tag: str | None,
    commits: list[str],
    stat: str,
    files: list[str],
    model: str,
) -> str | None:
    api_key = os.environ.get("OPENAI_API_KEY", "").strip()
    if not api_key:
        return None

    prompt = textwrap.dedent(
        f"""
        You are writing GitHub release notes for the public repository {repo}.

        Release tag: {tag}
        Version: {version}
        Previous tag: {prev_tag or "none"}
        Diff summary: {stat or "n/a"}
        Most-touched paths:
        {chr(10).join(f"- {item}" for item in top_paths(files, limit=10)) or "- n/a"}

        Commits:
        {chr(10).join(f"- {item}" for item in commits[:80]) or "- Initial tagged release"}

        Write concise markdown release notes with these sections and nothing else:
        1. A title `# rzn-python-sandbox {version}`
        2. `## Highlights` with 3-6 bullets in plain English
        3. `## Install` with one bullet for macOS/Linux and one bullet for Windows
        4. `## What's Changed` with 4-12 bullets grounded in the commit list above

        Rules:
        - Do not mention AI, prompts, or that an LLM wrote the notes.
        - Do not invent PR numbers, issues, or contributors.
        - Keep the tone direct, technical, and readable.
        - Mention that the repo now ships GitHub release assets when that is supported by the commit list.
        """
    ).strip()

    payload = {
        "model": model,
        "input": prompt,
    }
    request = urllib.request.Request(
        "https://api.openai.com/v1/responses",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=90) as response:
            raw = response.read().decode("utf-8")
    except urllib.error.URLError:
        return None

    parsed = json.loads(raw)
    text = extract_output_text(parsed).strip()
    return text or None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate GitHub release notes from git history.")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--repo", required=True, help="GitHub repository in owner/name form.")
    parser.add_argument("--output", required=True)
    parser.add_argument("--model", default=os.environ.get("OPENAI_RELEASE_NOTES_MODEL", DEFAULT_MODEL))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    tag = args.tag.strip()
    version = tag[1:] if tag.startswith("v") else tag
    model = args.model.strip() or DEFAULT_MODEL
    prev_tag = previous_tag(tag)
    revision_range = rev_range(prev_tag, tag)
    commits = commit_lines(revision_range)
    files = changed_files(revision_range)
    stat = diff_stat(revision_range)

    notes = llm_notes(
        repo=args.repo,
        tag=tag,
        version=version,
        prev_tag=prev_tag,
        commits=commits,
        stat=stat,
        files=files,
        model=model,
    )
    if notes is None:
        notes = fallback_notes(
            repo=args.repo,
            tag=tag,
            version=version,
            prev_tag=prev_tag,
            commits=commits,
            stat=stat,
            files=files,
        )
    Path(args.output).write_text(notes.rstrip() + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
