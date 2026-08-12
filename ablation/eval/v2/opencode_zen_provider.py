"""OpenCode Zen adapter for exec003; construction and local validation make no network call."""
from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Callable

PROVIDER_ROUTE = "opencode-zen"
MODEL = "deepseek-v4-flash-free"
ENDPOINT = "https://opencode.ai/zen/v1/chat/completions"
CREDENTIAL_ENV = "SXR_OPENCODE_ZEN_API_KEY"
RETRYABLE_HTTP = frozenset((429, 500, 502, 503, 504))

class ProviderFailure(RuntimeError):
    """An explicit provider rejection; its classification is safe to persist."""
    def __init__(self, classification: str, reason: str, status: int | None = None):
        super().__init__(reason); self.classification, self.status = classification, status

class AmbiguousTransport(RuntimeError):
    """The request may have reached the provider; it must never be retried."""
    def __init__(self, reason: str): super().__init__(reason)

def _safe_reason(exc: BaseException) -> str:
    # Exception type, errno and text are useful; headers/body/credentials are not.
    errno = getattr(exc, "errno", None)
    text = str(exc).replace("\n", " ")[:500]
    return f"{type(exc).__name__}" + (f" errno={errno}" if errno is not None else "") + (f": {text}" if text else "")

class OpenCodeZenClient:
    provider_route = PROVIDER_ROUTE
    endpoint = ENDPOINT
    model = MODEL

    def __init__(self, model: str = MODEL, timeout: float = 300.0,
                 attempt_sink: Callable[[dict], None] | None = None,
                 opener: Callable = urllib.request.urlopen, sleep: Callable = time.sleep):
        # Deliberately assignments only: no credential read or transport construction.
        self.model, self.timeout, self._sink, self._opener, self._sleep = model, timeout, attempt_sink, opener, sleep

    def validate_local_configuration(self) -> dict:
        key = os.environ.get(CREDENTIAL_ENV, "")
        if not isinstance(key, str) or not key.strip():
            raise ProviderFailure("REJECTED_FATAL", f"{CREDENTIAL_ENV} is missing or empty")
        if any(c.isspace() for c in key):
            raise ProviderFailure("REJECTED_FATAL", f"{CREDENTIAL_ENV} contains whitespace")
        return {"configured": True, "model": self.model, "provider_route": self.provider_route,
                "endpoint": self.endpoint, "network_requests": 0}

    def set_attempt_sink(self, sink: Callable[[dict], None] | None) -> None:
        """Bind a single unit's durable journal immediately before its call."""
        self._sink = sink

    def _record(self, row: dict) -> None:
        if self._sink: self._sink(dict(row))

    def complete(self, system: str, user: str, requested_max_tokens: int, temperature: float) -> dict:
        key = os.environ.get(CREDENTIAL_ENV, "")
        if not key.strip():
            raise ProviderFailure("REJECTED_FATAL", f"{CREDENTIAL_ENV} is missing or empty")
        body = json.dumps({"model": self.model, "temperature": temperature,
                           "max_tokens": requested_max_tokens,
                           "messages": [{"role": "system", "content": system},
                                        {"role": "user", "content": user}]}).encode()
        retries = 0
        # Explicit error responses settle an attempt. Ambiguous failures do not.
        for number in range(1, 4):
            self._record({"attempt_num": number, "status": "INTENT", "classification": None})
            start = time.monotonic()
            req = urllib.request.Request(self.endpoint, data=body, headers={
                "Content-Type": "application/json", "Authorization": "Bearer " + key})
            try:
                with self._opener(req, timeout=self.timeout) as response:
                    status = getattr(response, "status", None) or response.getcode()
                    payload = json.loads(response.read())
                elapsed = round((time.monotonic() - start) * 1000.0, 3)
                choices = payload.get("choices") if isinstance(payload, dict) else None
                choice = choices[0] if isinstance(choices, list) and choices and isinstance(choices[0], dict) else None
                message = choice.get("message") if choice else None
                content = message.get("content") if isinstance(message, dict) else None
                if not isinstance(content, str):
                    self._record({"attempt_num": number, "status": "REJECTED", "classification": "REJECTED_FATAL", "http_status": status, "reason": "invalid chat-completions response"})
                    raise ProviderFailure("REJECTED_FATAL", "invalid chat-completions response", status)
                usage = payload.get("usage") or {}; details = usage.get("completion_tokens_details") or {}
                provenance = {"provider_route": PROVIDER_ROUTE, "retries": retries, "latency_ms": elapsed}
                for name, value in (("resolved_model", payload.get("model")), ("generation_id", payload.get("id") or payload.get("request_id")),
                                    ("finish_reason", choice.get("finish_reason")), ("prompt_tokens", usage.get("prompt_tokens")),
                                    ("completion_tokens", usage.get("completion_tokens")), ("total_tokens", usage.get("total_tokens")),
                                    ("reasoning_tokens", details.get("reasoning_tokens")), ("cost", usage.get("cost")), ("http_status", status)):
                    if value is not None: provenance[name] = value
                self._record({"attempt_num": number, "status": "DELIVERED", "classification": "DELIVERED", "http_status": status})
                return {"text": content, "provider": provenance}
            except urllib.error.HTTPError as exc:
                status = exc.code; classification = "REJECTED_RETRYABLE" if status in RETRYABLE_HTTP else "REJECTED_FATAL"
                self._record({"attempt_num": number, "status": "REJECTED", "classification": classification, "http_status": status, "reason": f"HTTP {status}"})
                if classification == "REJECTED_RETRYABLE" and number < 3:
                    retries += 1; self._sleep(0) ; continue
                raise ProviderFailure(classification, f"HTTP {status}", status) from exc
            except (urllib.error.URLError, TimeoutError, ConnectionResetError, OSError) as exc:
                reason = _safe_reason(exc)
                self._record({"attempt_num": number, "status": "AMBIGUOUS", "classification": "AMBIGUOUS_TRANSPORT", "reason": reason})
                raise AmbiguousTransport(reason) from exc
