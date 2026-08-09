"""M2-G: key isolation — evaluators cannot resolve sealed answer material."""

import os

from ..bundle import (build_bundle, scrub_env, probe_key_paths,
                      assert_bundle_sanitized, bundle_references_scorer_state,
                      KEY_PATHS)


def test_bundle_contains_no_key_material(tmp_path):
    b = build_bundle(str(tmp_path), "u1", {"case_id": "c1"},
                     {"case_id": "c1", "arm": "A",
                      "sections": {"sql": "s", "fts": "f"}},
                     {"backend": "simulated", "system_prompt": "diagnose"})
    tree = []
    for root, _, files in os.walk(b.path):
        for f in files:
            tree.append(os.path.relpath(os.path.join(root, f), b.path))
    assert "keys.json" not in tree
    assert "scenario-keys.json" not in tree
    for kp in KEY_PATHS:
        assert not os.path.exists(os.path.join(b.path, os.path.basename(kp)))


def test_bundle_manifest_has_no_locus_or_scorer_refs(tmp_path):
    b = build_bundle(str(tmp_path), "u1", {"case_id": "c1"},
                     {"case_id": "c1", "arm": "A",
                      "sections": {"sql": "s", "fts": "f"}},
                     {"backend": "simulated", "system_prompt": "diagnose"})
    assert not bundle_references_scorer_state(b)


def test_scrub_env_removes_secrets_and_host_paths(tmp_path):
    b = build_bundle(str(tmp_path), "u1", {"case_id": "c1"},
                     {"case_id": "c1", "arm": "A",
                      "sections": {"sql": "s", "fts": "f"}},
                     {"backend": "simulated", "system_prompt": "diagnose"})
    env = {
        "HOME": "/home/sprime01",
        "PATH": "/home/sprime01/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
        "SXR_HOME": "/home/sprime01/.local/share/sxr",
        "OPENAI_API_KEY": "sk-secret",
        "PYTHONPATH": "/home/sprime01/projects/sxr/ablation/corpus",
        "KEEP": "hello",
    }
    scrubbed = scrub_env(env, b.path)
    assert "SXR_HOME" not in scrubbed
    assert "OPENAI_API_KEY" not in scrubbed
    assert "PYTHONPATH" not in scrubbed
    assert scrubbed["HOME"] == b.path
    assert scrubbed["PATH"] == "/usr/bin:/bin"
    assert scrubbed["KEEP"] == "hello"


def test_negative_probe_no_key_paths_resolve_from_bundle(tmp_path):
    """Automated negative test: an evaluator inside the bundle cannot resolve
    ANY known key path, relative or absolute, via cwd or env."""
    b = build_bundle(str(tmp_path), "u1", {"case_id": "c1"},
                     {"case_id": "c1", "arm": "A",
                      "sections": {"sql": "s", "fts": "f"}},
                     {"backend": "simulated", "system_prompt": "diagnose"})
    # place the sealed keys at the REAL corpus location to make the test
    # meaningful: the keys exist on this machine.
    real_keys = "/home/sprime01/projects/sxr/ablation/corpus/keys.json"
    if os.path.exists(real_keys):
        env = scrub_env({"HOME": "/home/sprime01", "PATH": os.environ["PATH"]},
                        b.path)
        resolved = probe_key_paths(env, b.path)
        assert resolved == [], f"evaluator resolved {resolved}"
        assert_bundle_sanitized(b, env)
