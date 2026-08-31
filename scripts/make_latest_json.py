#!/usr/bin/env python3
"""生成更新通道 latest.json（发版第 5 步的自动化替身）。

用法：
    python scripts/make_latest_json.py <version> <setup_exe> <setup_sig> [notes...]
输出 ./latest.json（Tauri v2 updater schema）。文件名必须是 latest.json 本身——
更新通道按此名拉取（HANDOVER §7.5 / 坑 R1）。
"""
import datetime
import io
import json
import sys

REPO = "yusheng266186-beep/zdisk-cleaner-pro"


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 1
    version = sys.argv[1].lstrip("v")
    setup, sig = sys.argv[2], sys.argv[3]
    notes = " ".join(sys.argv[4:]) or f"ZDiskCleaner Pro v{version}"
    asset = f"ZDiskCleanerPro_{version}_x64-setup.exe"
    assert setup.replace("\\", "/").endswith(asset), f"资产名必须精确为 {asset}，实际 {setup}"
    signature = io.open(sig, encoding="utf-8").read().strip()
    assert signature.startswith("dW50cnVzdGVk"), "签名内容异常（应为 minisign base64）"
    doc = {
        "version": version,
        "notes": notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "windows-x86_64": {
                "url": f"https://github.com/{REPO}/releases/download/v{version}/{asset}",
                "signature": signature,
            }
        },
    }
    out = "latest.json"
    io.open(out, "w", encoding="utf-8", newline="\n").write(json.dumps(doc, ensure_ascii=False, indent=2) + "\n")
    print(f"wrote {out} (version={version})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
