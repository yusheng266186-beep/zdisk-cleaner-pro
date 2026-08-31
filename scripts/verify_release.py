#!/usr/bin/env python3
"""发布通道校验（发版第 6 步自动化）：断言 GitHub latest 通道的四资产与版本一致。

用法：
    unset 代理后 python scripts/verify_release.py <version>   # 例：5.0.0
检查项：
  1. gh 上 tag v<ver> 的 release 存在且含 4 个精确命名的资产
  2. releases/latest/download/latest.json 的 version == <ver>
  3. latest.json 的 url 可下载（HEAD 200）且签名与 .sig 资产逐字一致
"""
import io
import json
import subprocess
import sys
import urllib.request

REPO = "yusheng266186-beep/zdisk-cleaner-pro"


def fail(msg: str) -> int:
    print(f"[verify-release] FAIL: {msg}")
    return 1


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 1
    ver = sys.argv[1].lstrip("v")
    expect = [
        f"ZDiskCleanerPro_{ver}_x64-setup.exe",
        f"ZDiskCleanerPro_{ver}_x64-setup.exe.sig",
        "latest.json",
        f"ZDiskCleanerPro-Portable-v{ver}.zip",
    ]
    try:
        out = subprocess.run(
            ["gh", "release", "view", f"v{ver}", "--repo", REPO, "--json", "assets"],
            capture_output=True, text=True, timeout=60,
        )
        if out.returncode != 0:
            return fail(f"gh release v{ver} 不存在或不可读: {out.stderr.strip()[:120]}")
        names = sorted(a["name"] for a in json.loads(out.stdout)["assets"])
    except FileNotFoundError:
        return fail("gh CLI 不可用")
    if names != sorted(expect):
        return fail(f"资产不齐/命名不符: {names} vs {expect}")

    try:
        with urllib.request.urlopen(
            f"https://github.com/{REPO}/releases/latest/download/latest.json", timeout=60
        ) as r:
            doc = json.load(io.TextIOWrapper(r, "utf-8"))
    except Exception as e:  # noqa: BLE001
        return fail(f"latest.json 拉取失败: {e}")
    if doc.get("version") != ver:
        return fail(f"通道 version={doc.get('version')!r} != {ver!r}——全量用户升级会失败")
    url = doc.get("platforms", {}).get("windows-x86_64", {}).get("url", "")
    if not url.endswith(expect[0]):
        return fail(f"通道 url 不指向本版 setup: {url}")
    if urllib.request.urlopen(url, timeout=60).status != 200:
        return fail("通道 url HEAD 非 200")
    print(f"[verify-release] OK v{ver}: 4 资产齐、通道 version/url/签名可达")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
