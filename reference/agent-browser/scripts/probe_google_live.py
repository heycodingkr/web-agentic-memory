#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""
probe_google_live — 격리 인스턴스에서 구글을 열어 봇 감지(sorry/캡차) 페이지가 뜨는지와 브라우저 지문을 찍는다.
버전 간 비교용: PROBE_BUNDLE_SRC 로 번들을 바꿔 같은 조건에서 두 번 돌린다.

    PROBE_BUNDLE_SRC=/Applications/Arise.app PROBE_SCRIPT=scripts/probe_google_live.py bash scripts/probe_arise_isolated.sh
"""
import json
import os
import socket
import sys
import time


def sock_path():
    return open(os.path.join(os.environ.get("TMPDIR", "/tmp"), "arise-control.sock.path")).read().strip()


def ctl(obj, timeout=20):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(timeout)
    s.connect(sock_path())
    s.sendall((json.dumps(obj) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        c = s.recv(1 << 20)
        if not c:
            break
        buf += c
    s.close()
    return json.loads(buf.decode())


def ev(js):
    r = ctl({"cmd": "eval_sync", "code": js})
    return r.get("result") if r.get("ok") else f"<{r}>"


FP = """JSON.stringify({
 webdriver: navigator.webdriver, ua: navigator.userAgent, chrome: typeof window.chrome,
 plugins: navigator.plugins.length, langs: navigator.languages, platform: navigator.platform,
 hw: navigator.hardwareConcurrency, mem: navigator.deviceMemory, vendor: navigator.vendor,
 uadata: !!navigator.userAgentData, brands: navigator.userAgentData && navigator.userAgentData.brands.map(b=>b.brand+'/'+b.version),
 outer: [outerWidth, outerHeight], screen: [screen.width, screen.height], dpr: devicePixelRatio,
 permissions: typeof Notification !== 'undefined' ? Notification.permission : 'n/a'
})"""


def visit(url, wait=7):
    ctl({"cmd": "navigate", "url": url})
    time.sleep(wait)
    href = ev("location.href")
    title = ev("document.title")
    text = (ev("document.body ? document.body.innerText.slice(0, 400) : ''") or "").replace("\n", " | ")
    flagged = any(k in (href or "").lower() for k in ("/sorry/", "captcha", "recaptcha")) or any(
        k in (text or "").lower() for k in ("unusual traffic", "not a robot", "비정상적인 트래픽", "로봇이 아닙니다"))
    return href, title, text, flagged


def main():
    ver = ev("navigator.userAgent")
    print("UA:", ver)
    print("fingerprint:", ev(FP))
    bad = False
    for url in ("https://www.google.com/", "https://www.google.com/search?q=weather+seoul"):
        href, title, text, flagged = visit(url)
        print(f"\n▶ {url}\n  final : {href}\n  title : {title}\n  text  : {text[:220]}\n  bot-check page: {'YES ⛔' if flagged else 'no ✅'}")
        bad |= flagged
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
