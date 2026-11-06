"""Wait for approved release PR checks, then merge only the validated head."""

from __future__ import annotations

import json
import os
import re
import subprocess
import time
from pathlib import Path


def gh(*args: str):
    result = subprocess.run(["gh", *args], check=True, text=True, capture_output=True, timeout=120)
    return json.loads(result.stdout) if result.stdout.strip() else None


def validate_pr(pr: dict, repo: str, branch: str, sha: str) -> None:
    if (
        pr["state"] != "open"
        or pr["draft"]
        or pr["base"]["ref"] != "main"
        or pr["base"]["repo"]["full_name"] != repo
        or pr["head"]["repo"]["full_name"] != repo
        or pr["head"]["ref"] != branch
        or pr["head"]["sha"] != sha
    ):
        raise ValueError("Release PR changed or is not an open release PR against main; refusing to merge.")


def wait_for_checks(repo: str, branch: str, sha: str) -> None:
    deadline = time.monotonic() + 1800
    run_id = None
    while time.monotonic() < deadline:
        if run_id is None:
            runs = gh(
                "api",
                "--method",
                "GET",
                f"repos/{repo}/actions/workflows/check.yml/runs",
                "-f",
                "event=pull_request",
                "-f",
                f"branch={branch}",
                "-f",
                "per_page=100",
            )["workflow_runs"]
            matches = [run for run in runs if run["head_sha"] == sha]
            if matches:
                selected = max(matches, key=lambda run: run["id"])
                run_id = selected["id"]
                print(
                    f"Required checks (approve workflows on the release PR): {selected['html_url']}",
                    flush=True,
                )
        if run_id is not None:
            run = gh("api", f"repos/{repo}/actions/runs/{run_id}")
            if run["head_sha"] != sha or run["event"] != "pull_request":
                raise ValueError("Validation run does not match the release commit.")
            # GitHub represents a bot PR waiting for human approval as a completed run.
            # Approval restarts this same run; keep waiting rather than treating it as failure.
            if run["status"] == "completed" and run["conclusion"] != "action_required":
                if run["conclusion"] != "success":
                    raise ValueError(f"Required checks ended with {run['conclusion']}; release PR left open.")
                jobs = gh("api", f"repos/{repo}/actions/runs/{run_id}/jobs?per_page=100")["jobs"]
                for name in ("check",):
                    matching = [job for job in jobs if job["name"] == name]
                    if len(matching) != 1 or matching[0]["conclusion"] != "success":
                        raise ValueError(f"Required {name} job did not succeed; release PR left open.")
                return
        time.sleep(10)
    raise ValueError(
        "Timed out waiting for workflow approval/checks; release PR left open for manual recovery."
    )


def complete(repo: str, number: int, branch: str, sha: str, tag: str) -> str:
    if branch != f"release/{tag}" or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("Expected a stable release branch and tag.")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected a full release commit SHA.")
    endpoint = f"repos/{repo}/pulls/{number}"
    validate_pr(gh("api", endpoint), repo, branch, sha)
    files = gh("api", "--paginate", "--slurp", f"{endpoint}/files?per_page=100")
    changed = {item["filename"] for page in files for item in page}
    if changed != {"Cargo.toml", "Cargo.lock"}:
        raise ValueError("Release PR must change only Cargo.toml and Cargo.lock; refusing to merge.")
    wait_for_checks(repo, branch, sha)
    validate_pr(gh("api", endpoint), repo, branch, sha)
    # The merge API enforces repository rules and atomically checks the validated head SHA.
    result = gh(
        "api",
        "--method",
        "PUT",
        f"{endpoint}/merge",
        "-f",
        f"sha={sha}",
        "-f",
        "merge_method=squash",
        "-f",
        f"commit_title=chore(release): prepare {tag} (#{number})",
    )
    if not result.get("merged") or not re.fullmatch(r"[0-9a-f]{40}", result.get("sha", "")):
        raise ValueError("GitHub did not confirm the release merge; no tag will be created.")
    return result["sha"]


def main() -> None:
    merged = complete(
        os.environ["GITHUB_REPOSITORY"],
        int(os.environ["RELEASE_PR"]),
        os.environ["RELEASE_BRANCH"],
        os.environ["RELEASE_SHA"],
        os.environ["RELEASE_TAG"],
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as stream:
        stream.write(f"merge_sha={merged}\n")
    print(f"Merged release commit: {merged}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        if isinstance(error, ValueError):
            raise SystemExit(str(error)) from None
        raise SystemExit(
            "GitHub operation failed; inspect checks and branch rules. No rules were bypassed."
        ) from None
