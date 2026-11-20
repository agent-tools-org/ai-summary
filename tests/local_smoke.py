"""Smoke-test a built binary with local documents, terminal output, and hooks.
Uses isolated files and loopback HTTP only. Standard library required.
"""

from __future__ import annotations

from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import pty
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import zipfile


def pdf_fixture() -> bytes:
    stream = b"BT /F1 12 Tf 20 60 Td (PDF fixture text) Tj ET"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"\nendstream",
    ]
    data = bytearray(b"%PDF-1.4\n")
    offsets = []
    for index, value in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(b"%d 0 obj\n" % index + value + b"\nendobj\n")
    xref = len(data)
    data.extend(b"xref\n0 6\n0000000000 65535 f \n")
    for offset in offsets:
        data.extend(b"%010d 00000 n \n" % offset)
    data.extend(b"trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n%d\n%%%%EOF\n" % xref)
    return bytes(data)


def docx_fixture() -> bytes:
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("word/document.xml", "<w:document><w:body><w:p><w:r>"
                         "<w:t>DOCX fixture &amp; text 你好</w:t></w:r></w:p>"
                         "</w:body></w:document>")
    return buffer.getvalue()


class LocalHandler(BaseHTTPRequestHandler):
    mode = "complete"
    POST_COUNT = 0
    PAGES = {
        "/text": ("text/plain", ("Text fixture with <tag> and 你好. " * 10).encode()),
        "/html": ("text/html", ("<main><p>HTML fixture text</p></main>" * 10).encode()),
        "/fixture.pdf": ("application/pdf", pdf_fixture()),
        "/fixture.docx": ("application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                          docx_fixture()),
    }

    def log_message(self, fmt: str, *args: object) -> None:
        return

    def respond(self, body: bytes, media: str) -> None:
        self.send_response(200)
        self.send_header("Content-Type", media)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        media, body = self.PAGES[self.path]
        self.respond(body, media)

    def do_POST(self) -> None:
        type(self).POST_COUNT += 1
        payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if payload["stream"]:
            chunk = {"choices": [{"delta": {"content": "<think>hidden</think>Terminal answer"}}]}
            text = "data: " + json.dumps(chunk) + "\n\n"
            if self.mode == "complete":
                text += "data: [DONE]\n\n"
            self.respond(text.encode(), "text/event-stream")
        else:
            body = {"choices": [{"message": {"content": "Fixture summary"}}]}
            self.respond(json.dumps(body).encode(), "application/json")


class LocalCli:
    def __init__(self, binary: Path, directory: Path) -> None:
        self.binary = binary
        self.directory = directory
        self.env = {
            "HOME": str(directory), "PATH": "/nonexistent",
            "HTTP_PROXY": "http://127.0.0.1:9", "HTTPS_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "127.0.0.1",
        }

    def run(self, *args: str, text: str = "") -> subprocess.CompletedProcess[str]:
        return subprocess.run([str(self.binary), *args], env=self.env, input=text,
                              capture_output=True, text=True, timeout=20, check=False)

    def tty(self, *args: str, text: str = "") -> tuple[int, str, str]:
        master, slave = pty.openpty()
        try:
            result = subprocess.run([str(self.binary), *args], env=self.env, input=text,
                                    stdout=slave, stderr=subprocess.PIPE, text=True,
                                    timeout=20, check=False)
            # Darwin can discard queued terminal data when the last slave closes.
            # Keep the slave open and drain the master without waiting for EOF.
            os.set_blocking(master, False)
            chunks = bytearray()
            try:
                while chunk := os.read(master, 4096):
                    chunks.extend(chunk)
            except BlockingIOError:
                pass
            return result.returncode, chunks.decode(), result.stderr
        finally:
            os.close(master)
            os.close(slave)


def check_corpus(cli: LocalCli, url: str) -> dict[str, int]:
    started = time.monotonic()
    urls = [url + path for path in LocalHandler.PAGES]
    result = cli.run("--raw", "--json", "fetch", *urls)
    assert result.returncode == 0, result.stderr
    output = json.loads(result.stdout)
    assert output["status"] == "ok" and output["schema_version"] == 1
    pages = output["pages"]
    assert [page["url"] for page in pages] == urls
    assert "<tag>" in pages[0]["text"]
    assert "HTML fixture text" in pages[1]["text"]
    assert "PDF fixture text" in pages[2]["text"]
    assert "DOCX fixture & text 你好" in pages[3]["text"]
    return {"pages": len(pages), "payload_bytes": sum(len(page["text"].encode()) for page in pages),
            "elapsed_ms": round((time.monotonic() - started) * 1000)}


def check_cache_and_tty(cli: LocalCli, url: str) -> None:
    args = ["--api-url", url, "fetch", url + "/text", "-p", "question"]
    result = cli.run("--metadata", *args)
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["cached"] is False
    calls = LocalHandler.POST_COUNT
    code, text, stderr = cli.tty(*args)
    assert code == 0 and "Fixture summary" in text and "cache hit" in stderr, (code, text, stderr)
    assert LocalHandler.POST_COUNT == calls
    bypassed = cli.run("--metadata", "--no-cache", *args)
    assert bypassed.returncode == 0 and not json.loads(bypassed.stdout)["cached"]
    entry = next((cli.directory / ".ai-summary/cache").iterdir())
    payload = json.loads(entry.read_text())
    payload["timestamp"] = 1
    entry.write_text(json.dumps(payload))
    refreshed = cli.run("--metadata", *args)
    assert refreshed.returncode == 0 and not json.loads(refreshed.stdout)["cached"]
    source = LocalHandler.PAGES["/text"]
    LocalHandler.PAGES["/text"] = ("text/plain", b"Updated fixture source. " * 20)
    try:
        changed = cli.run("--metadata", *args)
        assert changed.returncode == 0 and not json.loads(changed.stdout)["cached"]
    finally:
        LocalHandler.PAGES["/text"] = source
    args = ["--api-url", url, "sum"]
    code, text, stderr = cli.tty(*args, text="Unicode fixture 你好")
    assert code == 0 and "Terminal answer" in text and "hidden" not in text, stderr
    LocalHandler.mode = "incomplete"
    code, text, stderr = cli.tty(*args, text="Unicode fixture 你好")
    assert code == 2 and text == "" and "ended before completion" in stderr


def check_hooks(cli: LocalCli) -> None:
    jq = shutil.which("jq")
    if not jq:
        print("Hook shell smoke skipped because jq is unavailable")
        return
    assert cli.run("init").returncode == 0
    hook_dir = cli.directory / ".claude/hooks"
    command_dir = cli.directory / "fixture-bin"
    command_dir.mkdir()
    (command_dir / "jq").symlink_to(jq)
    (command_dir / "ai-summary").symlink_to(cli.binary)
    env = {"HOME": str(cli.directory), "PATH": f"{command_dir}:/usr/bin:/bin"}

    def invoke(name: str, payload: dict[str, object]) -> str:
        path = hook_dir / f"ai-summary-{name}.sh"
        result = subprocess.run(["/bin/bash", str(path)], env=env,
                                input=json.dumps(payload), capture_output=True, text=True,
                                check=True, timeout=10)
        return result.stdout

    payload: dict[str, object] = {"tool_input": {"command": "cargo test", "timeout": 1000}}
    update = json.loads(invoke("prebash", payload))["hookSpecificOutput"]
    assert "permissionDecision" not in update
    assert update["updatedInput"] == {"command": "ai-summary wrap cargo test", "timeout": 1000}
    assert not invoke("prebash", {"tool_input": {"command": "cargo testing"}})
    payload = {"tool_input": {"query": "fixture"}, "session_id": "session-a"}
    assert json.loads(invoke("prewebsearch", payload))["hookSpecificOutput"]["permissionDecision"] == "deny"
    assert not invoke("prewebsearch", payload)
    payload["session_id"] = "session-b"
    assert invoke("prewebsearch", payload)
    assert cli.run("init", "--uninstall").returncode == 0


def main() -> None:
    binary = Path(sys.argv[1]).resolve()
    server = ThreadingHTTPServer(("127.0.0.1", 0), LocalHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix="ai-summary-local-smoke-") as directory:
            cli = LocalCli(binary, Path(directory))
            url = f"http://127.0.0.1:{server.server_port}"
            corpus = check_corpus(cli, url)
            check_cache_and_tty(cli, url)
            check_hooks(cli)
            print(json.dumps({"version": cli.run("--version").stdout.strip(),
                              "corpus": corpus, "terminal_and_cache": "passed"}))
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    main()
