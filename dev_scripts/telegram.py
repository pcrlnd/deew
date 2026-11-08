#!/usr/bin/env python3
"""Telegram build notifications for the Build workflow (stdlib only)."""
import argparse
import html
import json
import sys
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone

PLATFORMS = [
    ("🐧", ["deew", "deew-gui", "deew-web"]),
    ("🪟", ["deew.exe", "deew-gui.exe", "deew-web.exe"]),
    ("🍎", ["deew-macos", "deew-gui-macos", "deew-web-macos"]),
]


def components(args):
    parts = (["CLI"] if args.cli else []) + (["GUI", "Web"] if args.gui_web else [])
    return ", ".join(parts)


def built_platforms(args):
    return "🐧 🪟" + (" 🍎" if args.macos else "")


def latest_changelog(path):
    lines = open(path, encoding="utf-8").read().split("\n")
    headers = [i for i, line in enumerate(lines) if line.startswith("# deew ")]
    end = headers[1] if len(headers) > 1 else len(lines)
    return "\n".join(lines[:end]).rstrip() + "\n"


def duration(started_at):
    start = datetime.fromisoformat(started_at.replace("Z", "+00:00"))
    if start.tzinfo is None:
        start = start.replace(tzinfo=timezone.utc)
    secs = max(0, int((datetime.now(timezone.utc) - start).total_seconds()))
    h, rem = divmod(secs, 3600)
    m, s = divmod(rem, 60)
    out = (f"{h}h " if h else "") + (f"{m}m " if h or m else "") + f"{s}s"
    return out


def read_assets(value):
    try:
        with open(value, encoding="utf-8") as f:
            names = f.read().splitlines()
    except OSError:
        names = value.split(",")
    return {n.strip() for n in names if n.strip()}


def start_text(args):
    return (
        f"🔨 Build started for <b>deew {html.escape(args.version)}</b> ({components(args)})\n"
        f"{built_platforms(args)}\n"
        f'<a href="{html.escape(args.run_url)}">Open the run</a>'
    )


def done_caption(args):
    base = f"https://github.com/{args.repo}/releases"
    tag = html.escape(args.version, quote=True)
    link = lambda name: (
        f'<a href="{html.escape(f"{base}/download/{args.version}/{name}")}">{html.escape(name)}</a>'
    )
    info = built_platforms(args)
    if args.started_at:
        info += f" - ⏱️ {duration(args.started_at)}"
    assets = read_assets(args.assets)
    lines = []
    for emoji, names in PLATFORMS:
        found = [link(n) for n in names if n in assets]
        if found:
            lines.append(f"{emoji} " + " - ".join(found))
    return (
        f'✅ <b><a href="{html.escape(f"{base}/tag/{args.version}")}">deew {tag}</a></b>'
        f" is out! ({components(args)})\n{info}\n\n" + "\n".join(lines)
    ).rstrip()


def post(token, method, body, content_type):
    req = urllib.request.Request(
        f"https://api.telegram.org/bot{token}/{method}",
        data=body,
        headers={"Content-Type": content_type},
    )
    try:
        urllib.request.urlopen(req, timeout=60).read()
    except urllib.error.HTTPError as e:
        try:
            desc = json.loads(e.read()).get("description", "")
        except Exception:
            desc = ""
        sys.exit(f"Telegram error (HTTP {e.code}): {desc}")
    except urllib.error.URLError as e:
        sys.exit(f"Telegram request failed: {e.reason}")


def multipart(fields, filename, data):
    boundary = uuid.uuid4().hex
    out = b""
    for k, v in fields.items():
        out += (
            f'--{boundary}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'
        ).encode()
    out += (
        f'--{boundary}\r\nContent-Disposition: form-data; name="document"; '
        f'filename="{filename}"\r\nContent-Type: text/markdown\r\n\r\n'
    ).encode() + data + f"\r\n--{boundary}--\r\n".encode()
    return out, f"multipart/form-data; boundary={boundary}"


def main():
    p = argparse.ArgumentParser()
    p.add_argument("command", choices=["start", "done"])
    p.add_argument("--token", default="")
    p.add_argument("--chat", default="")
    p.add_argument("--version", required=True)
    for flag in ("cli", "gui-web", "macos"):
        p.add_argument(f"--{flag}", required=True, type=lambda s: s == "true")
    p.add_argument("--run-url", default="")
    p.add_argument("--started-at", default="")
    p.add_argument("--assets", default="")
    p.add_argument("--repo", default="")
    p.add_argument("--changelog", default="changelog.md")
    p.add_argument("--dry-run", action="store_true")
    args = p.parse_args()

    if args.command == "start":
        text = start_text(args)
        if args.dry_run:
            print(text)
            return
        body = json.dumps(
            {
                "chat_id": args.chat,
                "text": text,
                "parse_mode": "HTML",
                "link_preview_options": {"is_disabled": True},
            }
        ).encode()
        post(args.token, "sendMessage", body, "application/json")
        return

    caption = done_caption(args)
    data = latest_changelog(args.changelog).encode("utf-8")
    if args.dry_run:
        print(caption)
        print(f"\n[document: changelog.md, {len(data)} bytes]")
        return
    body, ctype = multipart(
        {"chat_id": args.chat, "parse_mode": "HTML", "caption": caption},
        "changelog.md",
        data,
    )
    post(args.token, "sendDocument", body, ctype)


if __name__ == "__main__":
    main()
