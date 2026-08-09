"""M3 live model client — the ONLY component that contacts a model.

Reads the provider credential from opencode's auth store internally and never
prints or logs it. Performs OpenAI-compatible chat completions against the
frozen cline-api endpoint. Captures response text, token usage, gateway cost,
latency, generation id, and retry count.

Retry policy (predeclared): transport errors, HTTP 5xx, and HTTP 429 are
retried up to MAX_RETRIES with exponential backoff; any other HTTP 4xx is a
non-retryable error. A successful HTTP response is returned as-is even if the
content is empty or verbose — parser/format outcomes are DATA, not retries.
"""

from __future__ import annotations

import json
import os
import time
import urllib.request
import urllib.error
from dataclasses import dataclass

ENDPOINT = "https://api.cline.bot/api/v1/chat/completions"
AUTH_FILE = "~/.local/share/opencode/auth.json"
MAX_RETRIES = 3
RETRY_STATUS = (429, 500, 502, 503, 504)
RETRY_BACKOFF = (1.0, 2.0, 4.0)


class ModelClientError(Exception):
    """Non-retryable failure (bad auth, bad request, network exhausted)."""


@dataclass
class ModelCall:
    text: str
    usage: dict
    cost: float
    latency_ms: float
    generation_id: str | None
    retries: int
    model: str


def _read_key() -> str:
    """Return the cline-pass API key without ever printing it."""
    path = os.path.expanduser(AUTH_FILE)
    if not os.path.exists(path):
        raise ModelClientError(f"credential store missing: {path}")
    with open(path) as f:
        auth = json.load(f)
    try:
        key = auth["cline-pass"]["key"]
    except (KeyError, TypeError):
        raise ModelClientError("no 'cline-pass' credential in auth store")
    if not key:
        raise ModelClientError("empty cline-pass credential")
    return key


def call_model(model: str, system_prompt: str, user_prompt: str,
               temperature: float = 0.0, max_tokens: int = 4096,
               timeout: float = 120.0) -> ModelCall:
    """Execute one model call and return the parsed result + resource usage."""
    key = _read_key()
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
    headers = {
        "Content-Type": "application/json",
        "Authorization": "Bearer " + key,
    }

    last_err: Exception | None = None
    retries = 0
    return_obj: dict | None = None
    for attempt in range(MAX_RETRIES + 1):
        t0 = time.monotonic()
        req = urllib.request.Request(ENDPOINT, data=payload, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                raw = r.read().decode()
            return_obj = json.loads(raw)
            elapsed = (time.monotonic() - t0) * 1000.0
            break
        except urllib.error.HTTPError as e:
            elapsed = (time.monotonic() - t0) * 1000.0
            if e.code == 401:
                raise ModelClientError("cline-api auth rejected (401)")
            if e.code in RETRY_STATUS and attempt < MAX_RETRIES:
                retries += 1
                time.sleep(RETRY_BACKOFF[attempt])
                continue
            last_err = ModelClientError(f"HTTP {e.code}: {e.read().decode()[:300]}")
            break
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            elapsed = (time.monotonic() - t0) * 1000.0
            if attempt < MAX_RETRIES:
                retries += 1
                time.sleep(RETRY_BACKOFF[attempt])
                continue
            last_err = ModelClientError(f"transport error: {e}")
            break

    if return_obj is None:
        raise ModelClientError(f"model call failed after retries: {last_err}")

    data = return_obj.get("data", return_obj)
    choices = data.get("choices") or []
    if not choices:
        raise ModelClientError("model returned no choices")
    text = (choices[0].get("message") or {}).get("content") or ""
    usage = data.get("usage") or {}
    cost = float(usage.get("cost") or 0.0)
    gen = data.get("generationId") or choices[0].get("provider_metadata", {}).get(
        "gateway", {}).get("generationId")
    return ModelCall(
        text=text, usage=usage, cost=cost, latency_ms=round(elapsed, 1),
        generation_id=gen, retries=retries, model=data.get("model") or model,
    )