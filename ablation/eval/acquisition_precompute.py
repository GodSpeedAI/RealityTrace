#!/usr/bin/env python3
"""Precompute all probe observations for one case against its faulted state.

Called by precompute_probes.sh WHILE the scenario fault is injected and BEFORE
restore_baseline runs.  Every probe is a REAL read-only operation against the
frozen faulted repo working tree.  Results are structured findings (not raw
source/shell output) to prevent trivial discrimination via construction markers
while remaining genuine executed observations.

Usage:
    python3 acquisition_precompute.py \
        <repo_name> <scenario_num> <repo_path> <case_id> \
        '<envs>|<cmd>|<stdin_format>|<signature>'

Outputs one JSON object to stdout: {case_id, probes: {id: finding, ...}, hashes}
"""

from __future__ import annotations

import json
import os
import re
import stat
import subprocess
import sys
import time
from pathlib import Path

DF_BIN = os.environ.get(
    "SXR_DOMAINFORGE_BIN",
    "/home/sprime01/projects/domainforge-slice0b/target/debug/domainforge")


def _run(cmd: str, cwd: str, env: dict | None = None, timeout: float = 120.0):
    """Run a command, return (exit_code, stdout, stderr)."""
    try:
        r = subprocess.run(
            cmd, shell=True, cwd=cwd, env=env,
            capture_output=True, text=True, timeout=timeout)
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired:
        return 124, "", "TIMEOUT"
    except Exception as e:
        return 125, "", str(e)


def _parse_verifier_info(vinfo: str) -> dict:
    """Parse 'envs|cmd|stdin_format|signature' from scenario_verifier."""
    parts = vinfo.split("|")
    envs = parts[0].strip() if len(parts) > 0 else ""
    cmd = parts[1].strip() if len(parts) > 1 else ""
    stdin_format = parts[2].strip() if len(parts) > 2 else ""
    signature = parts[3].strip() if len(parts) > 3 else ""
    return {"envs": envs, "cmd": cmd, "stdin_format": stdin_format,
            "signature": signature}


def _make_env(base_env: dict, env_assignments: str) -> dict:
    """Apply 'KEY=VAL KEY2=VAL2' env assignments onto a copy of base env."""
    env = dict(base_env)
    for assignment in env_assignments.split():
        if "=" in assignment:
            k, v = assignment.split("=", 1)
            env[k] = v
    return env


# ── P1: inspect_target_binding ──────────────────────────────────────────────

def probe_target_binding(repo_path: str, vinfo: dict) -> dict:
    """Check if the test target/path referenced by the verifier exists."""
    cmd = vinfo["cmd"]
    findings = []
    rp = Path(repo_path)
    # cargo: --test <name> means tests/<name>.rs at any depth in the repo
    for m in re.finditer(r"--test\s+(\S+)", cmd):
        target = m.group(1)
        candidates = list(rp.glob(f"**/tests/{target}.rs"))
        exists = len(candidates) > 0
        findings.append({"target": f"tests/{target}.rs", "exists": exists})
    # pytest: explicit test file path
    for m in re.finditer(r"(?<!-)(\b\w[\w/]*\.py)\b", cmd):
        target = m.group(1)
        exists = (rp / target).exists()
        findings.append({"target": target, "exists": exists})
    # df-validate: .sea file path
    for m in re.finditer(r"(\S+\.sea)\b", cmd):
        target = m.group(1)
        exists = (rp / target).exists()
        findings.append({"target": target, "exists": exists})
    # If no specific target referenced, check that standard test dirs exist
    if not findings:
        has_tests = any(rp.glob("tests/*.rs")) or any(rp.glob("crates/*/tests/*.rs"))
        findings.append({"target": "(default test discovery)", "exists": has_tests})
    missing = [f for f in findings if f["exists"] is False]
    return {
        "checked": findings,
        "all_exist": len(missing) == 0,
        "missing_targets": [f["target"] for f in missing],
    }


# ── P2: inspect_source_symbols ──────────────────────────────────────────────

def probe_source_symbols(repo_path: str, vinfo: dict,
                         failure_output: str) -> dict:
    """Check if symbols referenced in the failure trace exist in source."""
    symbols = set()
    # cargo: various error formats for missing symbols
    for pat in [
        r"cannot find function `([^`]+)`",
        r"cannot find value `([^`]+)`",
        r"cannot find type `([^`]+)`",
        r"no method named `([^`]+)`",
        r"no function or associated item named `([^`]+)`",
        r"no associated item named `([^`]+)`",
        r"unresolved import `[^`]*::([^`:]+)`",   # path::to::Symbol
        r"unresolved import `([^`]+)`",
        r"no `([^`]+)` in `[a-z]",                 # rendered: no `Sym` in `mod`
    ]:
        symbols.update(re.findall(pat, failure_output))
    # pytest: "No module named 'name'", "cannot import name 'name'"
    for pat in [r"No module named '([^']+)'",
                r"cannot import name '([^']+)'"]:
        symbols.update(re.findall(pat, failure_output))
    if not symbols:
        return {"checked_symbols": [], "missing": [], "present": [],
                "note": "no symbols referenced in failure trace"}
    missing, present = [], []
    for sym in sorted(symbols):
        # Filter out very short or non-symbol matches
        if len(sym) < 3 or sym in ("self", "super", "crate", "std", "core"):
            continue
        found = False
        for ext in ("*.rs", "*.py"):
            for src_file in Path(repo_path).rglob(ext):
                sp = str(src_file)
                if "target/" in sp or "__pycache__" in sp:
                    continue
                # Skip test files — they reference symbols but don't define them
                if "/tests/" in sp or "/test_" in sp or sp.endswith("test.rs"):
                    continue
                try:
                    text = src_file.read_text(errors="replace")
                except (OSError, PermissionError):
                    continue
                # Check for DEFINITION patterns, not just mentions
                esc = re.escape(sym)
                if re.search(
                    rf"(?:pub\s+)?(?:async\s+)?fn\s+{esc}\b|"
                    rf"(?:pub\s+)?struct\s+{esc}\b|"
                    rf"(?:pub\s+)?enum\s+{esc}\b|"
                    rf"(?:pub\s+)?type\s+{esc}\b|"
                    rf"(?:pub\s+)?const\s+{esc}\b|"
                    rf"(?:pub\s+)?mod\s+{esc}\b|"
                    rf"def\s+{esc}\b|"
                    rf"class\s+{esc}\b", text):
                    found = True
                    break
            if found:
                break
        (present if found else missing).append(sym)
    return {"checked_symbols": sorted(symbols), "missing": missing,
            "present": present}


# ── P3: inspect_environment ─────────────────────────────────────────────────

def probe_environment(repo_path: str, vinfo: dict) -> dict:
    """Check toolchain availability and working-directory writability."""
    envs = vinfo["envs"]
    cmd = vinfo["cmd"]
    base = dict(os.environ)
    faulted_env = _make_env(base, envs)
    # Determine which toolchain to check from the verifier command
    if "cargo test" in cmd:
        check_cmd = "cargo --version"
    elif "uv run pytest" in cmd:
        check_cmd = "uv run pytest --version"
    elif "python3 -m pytest" in cmd:
        check_cmd = "python3 -m pytest --version"
    elif "domainforge" in cmd or DF_BIN in cmd:
        check_cmd = f'"{DF_BIN}" --help'
    else:
        check_cmd = "true"  # unknown — don't block
    rc, _, _ = _run(check_cmd, repo_path, env=faulted_env, timeout=15.0)
    toolchain = "available" if rc == 0 else "unavailable"
    # Check TMPDIR writability if set
    tmpdir_status = "default"
    tmpdir = faulted_env.get("TMPDIR")
    if tmpdir:
        if os.path.isdir(tmpdir) and os.access(tmpdir, os.W_OK):
            tmpdir_status = "writable"
        else:
            tmpdir_status = "not_writable"
    # Check PATH restriction
    path_status = "default"
    if "PATH=/usr/bin:/bin" in envs:
        path_status = "restricted"
    return {"toolchain": toolchain, "tmpdir": tmpdir_status,
            "path": path_status,
            "env_assignments": envs or "(none)"}


# ── P4: inspect_permissions ─────────────────────────────────────────────────

def probe_permissions(repo_path: str, vinfo: dict, failure_output: str) -> dict:
    """Check filesystem permissions on referenced artifacts."""
    paths_to_check = set()
    # Extract paths from verifier command
    for m in re.finditer(r"(\S+\.(?:rs|py|sea|toml|json))", vinfo["cmd"]):
        paths_to_check.add(m.group(1))
    # Extract paths from fixture references in failure output
    for m in re.finditer(r"['\"]((?:tests/|data/|crates/|src/|domainforge-|examples/)\S+['\"]?)",
                         failure_output):
        paths_to_check.add(m.group(1).strip("'\""))
    # Also check common source files that could be chmod'd
    for f in ("src/lib.rs", "src/main.rs", "src/lib.py",
              "domainforge-core/src/lib.rs",
              "crates/sea-forge-core/src/lib.rs",
              "tests/fixtures/example.py",
              "tests/fixtures/sample_settlement_recorded.json"):
        if (Path(repo_path) / f).exists():
            paths_to_check.add(f)
    findings = []
    for rel in sorted(paths_to_check):
        full = Path(repo_path) / rel
        if full.exists() or full.is_symlink():
            try:
                st = full.lstat()
                mode = stat.S_IMODE(st.st_mode)
                findings.append({"path": rel, "mode": oct(mode)[2:],
                                 "readable": os.access(full, os.R_OK),
                                 "restrictive": mode == 0})
            except OSError:
                findings.append({"path": rel, "mode": "error",
                                 "readable": False, "restrictive": True})
    restrictive = [f for f in findings if f.get("restrictive")]
    return {"checked": findings, "restrictive_paths": restrictive,
            "all_accessible": len(restrictive) == 0}


# ── P5: inspect_fixture_integrity ───────────────────────────────────────────

def probe_fixture_integrity(repo_path: str, vinfo: dict,
                            failure_output: str) -> dict:
    """Check if referenced fixtures exist and have expected content."""
    fixtures = set()
    # Extract fixture/data paths from failure output (clean trailing chars)
    for m in re.finditer(r"['\"]((?:tests/fixtures/|data/)[\w./-]+)", failure_output):
        fixtures.add(m.group(1))
    # Also scan TEST SOURCE files for fixture/data references
    for ext in ("*.rs", "*.py"):
        for test_file in Path(repo_path).rglob(ext):
            sp = str(test_file)
            if "target/" in sp or "__pycache__" in sp:
                continue
            if "/tests/" not in sp and "/test_" not in sp:
                continue
            try:
                text = test_file.read_text(errors="replace")
            except (OSError, PermissionError):
                continue
            for m in re.finditer(r'["\']((?:tests/fixtures/|data/)[\w./-]+)', text):
                fixtures.add(m.group(1))
    # Also check well-known fixture locations
    for d in ("tests/fixtures", "data"):
        dpath = Path(repo_path) / d
        if dpath.is_dir():
            try:
                for f in dpath.iterdir():
                    if f.is_file():
                        fixtures.add(str(f.relative_to(repo_path)))
            except (OSError, PermissionError):
                pass  # directory not accessible — P4 will report restrictive
    if not fixtures:
        return {"checked": [], "missing": [], "stale": [], "ok": [],
                "note": "no fixtures referenced"}
    missing, stale, ok = [], [], []
    for rel in sorted(fixtures):
        full = Path(repo_path) / rel
        if not full.exists():
            missing.append(rel)
            continue
        try:
            content = full.read_text(errors="replace")
        except (OSError, PermissionError):
            stale.append(rel)
            continue
        # Check for stale markers
        if "stale" in content.lower():
            stale.append(rel)
        else:
            ok.append(rel)
    return {"checked": sorted(fixtures), "missing": missing,
            "stale": stale, "ok": ok}


# ── P6: validate_semantic_model ─────────────────────────────────────────────

def probe_validate_model(repo_path: str, vinfo: dict) -> dict:
    """Validate the .sea model against schema/registry."""
    # Find the .sea file from the verifier command or repo config
    sea_file = None
    for m in re.finditer(r"(\S+\.sea)\b", vinfo["cmd"]):
        sea_file = m.group(1)
        break
    if not sea_file:
        # Check default locations
        for candidate in (".sea/interaction/interaction-model.sea",
                          "examples/namespaces/finance/cashflow.sea"):
            if (Path(repo_path) / candidate).exists():
                sea_file = candidate
                break
    if not sea_file:
        return {"valid": None, "errors": [], "note": "no .sea model in repo"}
    full = Path(repo_path) / sea_file
    if not full.exists():
        return {"valid": False, "errors": [f"{sea_file} not found"],
                "model_file": sea_file}
    rc, stdout, stderr = _run(
        f'"{DF_BIN}" validate "{sea_file}"', repo_path, timeout=30.0)
    errors = []
    if rc != 0:
        combined = (stdout + "\n" + stderr).strip()
        errors = [line.strip() for line in combined.split("\n")
                  if line.strip()][:5]
    return {"valid": rc == 0, "errors": errors, "model_file": sea_file,
            "exit_code": rc}


# ── P7: rerun_verifier_isolated ─────────────────────────────────────────────

def probe_rerun_isolated(repo_path: str, vinfo: dict,
                         failure_output: str) -> dict:
    """Re-run the specific failing test in isolation."""
    cmd = vinfo["cmd"]
    stdin_format = vinfo["stdin_format"]
    envs = vinfo["envs"]
    base = dict(os.environ)
    faulted_env = _make_env(base, envs) if envs else base

    # Determine the isolated test name from the failure output
    test_name = None
    if stdin_format == "car-go-json":
        # Extract failing test from cargo output
        m = re.search(r'"name"\s*:\s*"([^"]*corpus_scenario[^"]*)"', failure_output)
        if m:
            test_name = m.group(1)
        else:
            m = re.search(r"test\s+(\S*corpus\S*)\s+\.\.\. FAILED", failure_output)
            if m:
                test_name = m.group(1)
    elif stdin_format == "pytest-progress":
        m = re.search(r"FAILED\s+(\S+::\S+)", failure_output)
        if m:
            test_name = m.group(1)
        else:
            m = re.search(r"(test_corpus_scenario\w*)", failure_output)
            if m:
                test_name = m.group(1)
    elif stdin_format == "df-validate":
        # df-validate has a single "test": the validation itself
        test_name = "(validate)"

    # Build isolated rerun command: run ONLY the failing test, default env
    isolated_cmd = None
    if test_name and test_name != "(validate)":
        if stdin_format == "car-go-json":
            # Use the same cargo test command but with specific test name
            # and NO env restrictions (clean env)
            isolated_cmd = re.sub(
                r"corpus_scenario\S*", test_name, cmd)
            isolated_cmd = re.sub(r"--message-format json", "", isolated_cmd)
            isolated_cmd = isolated_cmd.strip()
        elif stdin_format == "pytest-progress":
            # Run just the specific test
            test_file = test_name.split("::")[0] if "::" in test_name else None
            if test_file:
                isolated_cmd = f"uv run pytest -q {test_file}::{test_name.split('::')[-1]}"
    elif test_name == "(validate)":
        # Re-run validation with clean env
        isolated_cmd = cmd

    if not isolated_cmd:
        # Check if this was a compile error (no test extracted because
        # compilation failed before tests ran)
        is_compile_error = "error[E" in failure_output or "could not compile" in failure_output
        return {"test": test_name or "(none)",
                "isolated_result": "compile_error" if is_compile_error else "not_found",
                "summary": "compilation failed — no test could run" if is_compile_error
                else "could not determine isolated test"}

    # Run with CLEAN env (no env restrictions from the scenario)
    t0 = time.monotonic()
    rc, stdout, stderr = _run(isolated_cmd, repo_path, env=base, timeout=120.0)
    elapsed = time.monotonic() - t0
    combined = (stdout + "\n" + stderr).strip()
    # Determine pass/fail
    if rc == 0:
        result = "passed"
    elif rc == 124:
        result = "timeout"
    else:
        result = "failed"
    # Extract a brief summary
    summary_lines = combined.split("\n")
    summary = "\n".join(summary_lines[-5:]) if summary_lines else "(no output)"
    return {"test": test_name, "isolated_cmd": isolated_cmd,
            "isolated_result": result, "exit_code": rc,
            "elapsed_s": round(elapsed, 1), "summary": summary[:500]}


# ── P8: inspect_verifier_config ─────────────────────────────────────────────

def probe_verifier_config(repo_name: str, vinfo: dict) -> dict:
    """Report the verifier type, evidence types, and direction rule."""
    stdin_format = vinfo["stdin_format"]
    config = {
        "car-go-json": {
            "verifier": "cargo-test",
            "evidence_types": ["test_result", "execution_log"],
            "direction_rule": "exit_code: 0→supports, nonzero→contradicts",
        },
        "pytest-progress": {
            "verifier": "pytest",
            "evidence_types": ["test_result"],
            "direction_rule": "exit_code: 0→supports, nonzero→contradicts",
        },
        "df-validate": {
            "verifier": "domainforge-validate",
            "evidence_types": ["conformance_result"],
            "direction_rule": "exit_code: 0→valid, nonzero→invalid",
        },
    }
    result = config.get(stdin_format, {"verifier": "unknown"})
    result["verifier_command"] = vinfo["cmd"][:200]
    result["env_assignments"] = vinfo["envs"] or "(none)"
    return result


# ── main ────────────────────────────────────────────────────────────────────

def _preprocess_failure(failure_output: str, stdin_format: str) -> str:
    """Strip cargo JSON explanation text to prevent false-positive symbol matches."""
    if stdin_format != "car-go-json":
        return failure_output
    clean_lines = []
    for line in failure_output.split("\n"):
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
            if d.get("reason") == "compiler-message":
                msg = d.get("message") or {}
                # Keep only message + rendered, NOT the explanation
                clean_lines.append(msg.get("message", ""))
                clean_lines.append(msg.get("rendered") or "")
            elif d.get("reason") == "test-result":
                clean_lines.append(json.dumps(d, sort_keys=True))
        except (json.JSONDecodeError, ValueError):
            clean_lines.append(line)
    return "\n".join(clean_lines)


def main():
    repo_name = sys.argv[1]
    scenario_num = int(sys.argv[2])
    repo_path = sys.argv[3]
    case_id = sys.argv[4]
    vinfo_str = sys.argv[5] if len(sys.argv) > 5 else ""
    vinfo = _parse_verifier_info(vinfo_str)

    # Run the verifier to capture failure output (under faulted env)
    base = dict(os.environ)
    faulted_env = _make_env(base, vinfo["envs"]) if vinfo["envs"] else base
    failure_rc, failure_out, failure_err = _run(
        vinfo["cmd"], repo_path, env=faulted_env, timeout=180.0)
    raw_failure = (failure_out + "\n" + failure_err).strip()
    # Preprocess: strip cargo JSON explanation text to prevent false positives
    failure_output = _preprocess_failure(raw_failure, vinfo["stdin_format"])

    # Run all probes (each isolated so one crash doesn't fail the case)
    probes = {}
    probe_runners = [
        ("inspect_target_binding", lambda: probe_target_binding(repo_path, vinfo)),
        ("inspect_source_symbols", lambda: probe_source_symbols(repo_path, vinfo, failure_output)),
        ("inspect_environment", lambda: probe_environment(repo_path, vinfo)),
        ("inspect_permissions", lambda: probe_permissions(repo_path, vinfo, failure_output)),
        ("inspect_fixture_integrity", lambda: probe_fixture_integrity(repo_path, vinfo, failure_output)),
        ("validate_semantic_model", lambda: probe_validate_model(repo_path, vinfo)),
        ("rerun_verifier_isolated", lambda: probe_rerun_isolated(repo_path, vinfo, failure_output)),
        ("inspect_verifier_config", lambda: probe_verifier_config(repo_name, vinfo)),
    ]
    for pid, runner in probe_runners:
        try:
            probes[pid] = runner()
        except Exception as e:
            probes[pid] = {"error": f"{type(e).__name__}: {e}"}

    # Bind by content hash
    probes_json = json.dumps(probes, sort_keys=True, separators=(",", ":"))
    probes_hash = __import__("hashlib").sha256(probes_json.encode()).hexdigest()

    result = {
        "case_id": case_id,
        "repo": repo_name,
        "scenario_num": scenario_num,
        "verifier_exit": failure_rc,
        "failure_signature_present": bool(vinfo["signature"] and re.search(
            vinfo["signature"], raw_failure)) if vinfo["signature"] else None,
        "probes": probes,
        "probes_hash": probes_hash,
    }
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
