"""Z.ai GLM Coding Plan client — the §23.2 provider-route substitution client.

Calls GLM-5.2 through the DEDICATED Coding Plan endpoint
(https://api.z.ai/api/coding/paas/v4), NOT the general pay-as-you-go endpoint.
Uses the zai-coding-plan credential (Zhipu signed-JWT HS256 auth). Returns the
same ModelCall shape as model_client so the runner/scorer paths are unchanged.

This route separates reasoning_content (thinking) from content (final answer).
The harness commits `content` (the locus answer) as the raw output; reasoning
tokens are captured in usage for cost accounting.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request

from .model_client import ModelCall, ModelClientError

CODING_URL = "https://api.z.ai/api/coding/paas/v4/chat/completions"
AUTH_FILE = "~/.local/share/opencode/auth.json"
CODING_PLAN_KEY = "zai-coding-plan"
MAX_RETRIES = 3
RETRY_STATUS = (429, 500, 502, 503, 504)
RETRY_BACKOFF = (2.0, 4.0, 8.0)


def _b64u(b: bytes) -> bytes:
    return base64.urlsafe_b64encode(b).rstrip(b"=")


def _jwt(key: str) -> str:
    api_id, api_secret = key.split(".")
    now = int(round(time.time() * 1000))
    header = _b64u(json.dumps({"alg": "HS256", "sign_type": "SIGN"},
                              separators=(",", ":")).encode())
    payload = _b64u(json.dumps({"api_key": api_id, "exp": now + 3600 * 1000,
                                "timestamp": now},
                               separators=(",", ":")).encode())
    si = header + b"." + payload
    sig = _b64u(hmac.new(api_secret.encode(), si, hashlib.sha256).digest())
    return (si + b"." + sig).decode()


def _read_key() -> str:
    path = os.path.expanduser(AUTH_FILE)
    with open(path) as f:
        auth = json.load(f)
    try:
        return auth[CODING_PLAN_KEY]["key"]
    except (KeyError, TypeError):
        raise ModelClientError("no zai-coding-plan credential in auth store")


def call_coding_plan(model: str, system_prompt: str, user_prompt: str,
                     temperature: float = 0.0, max_tokens: int = 8192,
                     timeout: float = 300.0) -> ModelCall:
    """One GLM-5.2 call via the Coding Plan endpoint. Returns ModelCall."""
    key = _read_key()
    token = _jwt(key)
    body = {
        "model": model,
        "temperature": temperature,
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
    }
    payload = json.dumps(body).encode()
    headers = {"Content-Type": "application/json", "Authorization": "Bearer " + token}

    last_err = None
    retries = 0
    data = None
    elapsed = 0.0
    for attempt in range(MAX_RETRIES + 1):
        t0 = time.monotonic()
        req = urllib.request.Request(CODING_URL, data=payload, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                data = json.loads(r.read())
            elapsed = (time.monotonic() - t0) * 1000.0
            break
        except urllib.error.HTTPError as e:
            elapsed = (time.monotonic() - t0) * 1000.0
            if e.code == 401:
                raise ModelClientError("z.ai coding-plan auth rejected (401)")
            # Coding-plan quota (429) is a real capacity signal; retry transient
            if e.code in RETRY_STATUS and attempt < MAX_RETRIES:
                retries += 1
                time.sleep(RETRY_BACKOFF[attempt])
                continue
            last_err = ModelClientError(
                f"HTTP {e.code}: {e.read().decode()[:300]}")
            break
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            elapsed = (time.monotonic() - t0) * 1000.0
            if attempt < MAX_RETRIES:
                retries += 1
                time.sleep(RETRY_BACKOFF[attempt])
                continue
            last_err = ModelClientError(f"transport error: {e}")
            break

    if data is None:
        raise ModelClientError(f"coding-plan call failed after retries: {last_err}")

    choices = data.get("choices") or []
    if not choices:
        raise ModelClientError("coding-plan returned no choices")
    msg = choices[0].get("message") or {}
    text = msg.get("content") or ""
    usage = data.get("usage") or {}
    reasoning_tokens = ((usage.get("completion_tokens_details") or {})
                        .get("reasoning_tokens", 0))
    usage = dict(usage)
    usage["reasoning_tokens"] = reasoning_tokens
    return ModelCall(
        text=text, usage=usage,
        cost=float(usage.get("cost") or 0.0),
        latency_ms=round(elapsed, 1),
        generation_id=data.get("id") or data.get("request_id"),
        retries=retries, model=data.get("model") or model)