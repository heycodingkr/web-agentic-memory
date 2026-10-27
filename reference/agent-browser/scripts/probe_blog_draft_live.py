#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""
probe_blog_draft_live — 블로그 초안 경로(폰트 scope·본문/제목 분리)를
**실제 Arise.app 소켓 + 실제 cef_host JS**로 라이브 검증한다. 대상은 네이버가 아니라
SE ONE 구조를 재현한 **mock 편집기**(assets/mock_se_editor.html).

## 왜 mock인가 (BCB #37886)
실 네이버 블로그 에디터(postwrite)는 로그인 세션이 필요하고, 그건 자동 로그인을
의미한다 — BCB #31864에 반복 시도로 계정이 잠긴 이력이 있다. 그래서 오케스트레이션은
mock으로 검증한다. 이 테스트는 **네이버와 아무 연락도 하지 않는다** — file:// 로컬 페이지.

## 이 테스트가 답하는 것
1. 버그 B: `querySelector('[contenteditable=true]')`가 실제로 **제목**을 잡는가
   (그리고 우리가 쓰는 명시 셀렉터는 **본문**을 잡는가) — 원인의 실증
2. 버그 B 수정: `type_text`가 본문에 들어가고 제목은 안 건드리는가
3. 버그 B 수정: `select_body`가 본문 편집기를 고르는가
4. 버그 C 수정: `font_size scope=body`가 **bodyFontSpans 델타**를 만드는가
   (제목에 이미 span이 있어도 오판하지 않는가)
5. 미결 질문: keyup이 keydown보다 적게 잡히는 게 **엔진 결함**인가 프로브 경쟁인가
   — mock 페이지가 키 값을 배열로 기록하므로 누락된 키를 정확히 지목할 수 있다
"""

import json
import os
import socket
import sys
import time

IO_TIMEOUT = 20.0

# 본문 포커스 + 계측 초기화.
# ⚠️ f-string 안에 넣으면 JS의 중괄호를 파이썬이 필드로 파싱해 문법 에러가 난다 —
# 반드시 상수로 분리할 것.
FOCUS_BODY_JS = r"""(() => {
  function reset(C){ C.kd.length=0; C.ku.length=0; C.inp=0; C.t0=0; C.t1=0; }
  // ⚠️ **제자리에서 비운다** — 새 객체로 치환하면 안 된다.
  // mock의 리스너가 `var C = window.__ie[slot]` 으로 **처음에 한 번** 캡처해 두고
  // 계속 그 객체에 쓰기 때문에, `window.__ie.body = {...}` 로 갈아끼우면 리스너는
  // 고아가 된 예전 객체에 쓰고 판독은 새(빈) 객체를 읽는다 → 항상 0.
  reset(window.__ie.body); reset(window.__ie.title);
  var b = document.querySelector('.se-section-container [contenteditable=true]');
  if (b) { b.focus(); return 'FOCUSED_BODY:' + b.id; }
  return 'NO_BODY';
})()"""


def sock_path() -> str:
    p = os.path.join(os.environ.get("TMPDIR", "/tmp"), "arise-control.sock.path")
    try:
        with open(p) as f:
            return f.read().strip()
    except OSError as e:
        print(f"⛔ 소켓 경로 파일 없음: {p} — {e}")
        sys.exit(2)


class Arise:
    """제어 소켓 — 요청마다 새 연결 (앱 쪽 serve()가 연결당 1줄 처리 후 닫는다)."""

    def __init__(self, path: str):
        self.path = path

    def send(self, obj: dict) -> str:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(IO_TIMEOUT)
        try:
            s.connect(self.path)
            s.sendall((json.dumps(obj, ensure_ascii=False) + "\n").encode())
            buf = b""
            while True:
                chunk = s.recv(65536)
                if not chunk:
                    break
                buf += chunk
        except OSError as e:
            return json.dumps({"ok": False, "error": f"소켓 통신 실패: {e}"}, ensure_ascii=False)
        finally:
            s.close()
        return buf.decode(errors="replace").strip()

    def eval_sync(self, code: str) -> str:
        raw = self.send({"cmd": "eval_sync", "code": code})
        try:
            v = json.loads(raw)
            return v.get("result", raw) if v.get("ok", False) else f"ERR:{v.get('error', raw)}"
        except json.JSONDecodeError:
            return raw

    def eval_json(self, code: str) -> dict:
        r = self.eval_sync(code)
        if isinstance(r, str) and r.startswith("ERR:"):
            return {"_err": r}
        try:
            return json.loads(r)
        except (json.JSONDecodeError, TypeError):
            return {"_raw": r}

    def blog_status(self) -> dict:
        raw = self.send({"cmd": "blog_status"})
        try:
            return json.loads(raw).get("status", {})
        except json.JSONDecodeError:
            return {"_raw": raw}

    def blog_cmd(self, op: str, **kw) -> str:
        payload = {"cmd": "blog_cmd", "op": op}
        payload.update(kw)
        return self.send(payload)


def wait_status(a, want, timeout=25.0, label=""):
    """blog_cmd / type_human 은 **fire-and-forget** 이다 — ack는 즉시 오고 실제 작업은
    앱 스레드가 나중에 수행한다. 고정 sleep으로 재면 그 비동기 작업과 **경쟁**해서
    값을 엉뚱한 시점에 읽는다. 그래서 상태를 **폴링**해 조건이 될 때까지 기다린다."""
    t0 = time.time()
    st = a.blog_status()
    while time.time() - t0 < timeout:
        if want(st):
            return st, (time.time() - t0) * 1000
        time.sleep(0.4)
        st = a.blog_status()
    return st, (time.time() - t0) * 1000


def counter(st: dict, key: str) -> int:
    v = st.get(key, 0)
    return v if isinstance(v, int) else 0


def main() -> None:
    print("[BLOG-DRAFT-LIVE] 블로그 초안 경로 라이브 검증")
    print("  대상: assets/mock_se_editor.html (SE ONE 구조 재현, **네이버 아님**)")
    print("  경로: probe → 제어 소켓 → cef_host blog_cmd → cadence_core → 실제 CEF 렌더러\n")

    path = sock_path()
    print(f"  제어 소켓: {path}")
    a = Arise(path)

    pong = a.send({"cmd": "ping"})
    print(f"[0] ping → {pong}")
    if '"pong":true' not in pong.replace(" ", ""):
        print("    ⛔ 앱 응답 없음 — 중단")
        return

    # ── mock 편집기 로드 ──
    html = os.path.abspath("assets/mock_se_editor.html")
    if not os.path.exists(html):
        print(f"⛔ mock 페이지 없음: {html}")
        return
    url = "file://" + html
    print(f"\n[1] mock 편집기 로드: {url}")
    print(f"    navigate → {a.send({'cmd': 'navigate', 'url': url})}")
    for _ in range(30):
        time.sleep(0.5)
        if a.eval_sync("(() => !!document.querySelector('.se-section-container'))()") == "true":
            break
    print(f"    현재 URL: {a.eval_sync('(() => location.href)()')}")

    # ── 2) DOM 구조 + 버그 B 원인 실증 ──
    print("\n[2] DOM 구조 — 버그 B 원인 실증")
    probe = a.eval_json(r"""(() => {
      var all = document.querySelectorAll('[contenteditable=true]');
      var naive = document.querySelector('[contenteditable=true]');
      var bodyExp = document.querySelector('.se-section-container [contenteditable=true]')
                 || document.querySelector('.se-section-container')
                 || (all.length ? all[all.length-1] : null);
      var titleExp = document.querySelector('.se-title-text [contenteditable=true]')
                  || document.querySelector('.se-title-text')
                  || (all.length ? all[0] : null);
      return JSON.stringify({
        contenteditable_count: all.length,
        dom_order: Array.prototype.map.call(all, function(e){return e.id}),
        naive_query_id: naive ? naive.id : null,
        explicit_title_id: titleExp ? titleExp.id : null,
        explicit_body_id: bodyExp ? bodyExp.id : null,
      });
    })()""")
    print(f"    {probe}")
    naive_is_title = probe.get("naive_query_id") == "mock-title"
    explicit_body_ok = probe.get("explicit_body_id") == "mock-body"
    explicit_title_ok = probe.get("explicit_title_id") == "mock-title"
    print(f"    ✅ contenteditable이 둘 이상: {probe.get('contenteditable_count')}개")
    print(f"    {'✅' if naive_is_title else '❌'} 순진한 셀렉터가 **제목**을 잡음 — 버그 B 원인 실증")
    print(f"    {'✅' if explicit_body_ok else '❌'} 명시 셀렉터는 **본문**을 잡음 (수정본)")
    print(f"    {'✅' if explicit_title_ok else '❌'} 명시 제목 셀렉터는 **제목**을 잡음")

    # ── 3) 기준선 ──
    print("\n[3] blog_status 기준선")
    base = a.blog_status()
    print(f"    {base}")
    b_title_len, b_body_len = counter(base, "titleLen"), counter(base, "bodyLen")
    b_title_fs, b_body_fs = counter(base, "titleFontSpans"), counter(base, "bodyFontSpans")
    print(f"    titleLen={b_title_len} bodyLen={b_body_len} "
          f"titleFontSpans={b_title_fs} bodyFontSpans={b_body_fs}")

    results = []

    # ── 4) title_text → 제목에만 반영 ──
    # ⚠️ 기준선은 **여기서** 잡는다 — title_text가 제목에 폰트 span을 만들기 때문에
    # 그 이전 값을 기준으로 잡으면 이후 판정이 전부 어긋난다.
    TITLE = "라이브검증 제목"
    BODY = "본문 문단 하나 — 이게 제목이 아니라 본문에 들어가야 한다"
    print(f"\n[4] blog_cmd title_text \"{TITLE}\" — 완료까지 폴링")
    print(f"    ack: {a.blog_cmd('title_text', text=TITLE)}")
    st, ms = wait_status(a, lambda s: counter(s, "titleLen") > 0, label="titleLen")
    print(f"    {ms:.0f}ms 만에 titleLen>0 도달")
    print(f"    {st}")
    ok = counter(st, "titleLen") > 0 and counter(st, "bodyLen") == 0
    results.append(("title_text가 제목에만 반영됨 (본문 불변)", ok))
    print(f"    {'✅' if ok else '❌'} 제목에만 반영")
    # ★ 기준선 — 이 시점부터의 변화로 판정한다
    base = st
    b_title_len, b_body_len = counter(base, "titleLen"), counter(base, "bodyLen")
    b_title_px, b_body_px = counter(base, "titleFontSize"), counter(base, "bodyFontSize")
    print(f"    ★ 기준선(타이틀 반영 후): titleLen={b_title_len} bodyLen={b_body_len} "
          f"titlePx={b_title_px} bodyPx={b_body_px}")

    # ── 5) type_text → **본문**에 반영돼야 함 (버그 B 수정 실증) ──
    print(f"\n[5] blog_cmd type_text \"{BODY}\" — 본문이어야 한다 (폴링)")
    print(f"    ack: {a.blog_cmd('type_text', text=BODY)}")
    st, ms = wait_status(a, lambda s: counter(s, "bodyLen") > b_body_len, label="bodyLen")
    print(f"    {ms:.0f}ms 만에 bodyLen 증가 도달")
    t_len2, bo_len2 = counter(st, "titleLen"), counter(st, "bodyLen")
    print(f"    titleLen {b_title_len}→{t_len2}, bodyLen {b_body_len}→{bo_len2}")
    body_grew = bo_len2 > b_body_len
    title_untouched = t_len2 == b_title_len
    results.append(("type_text가 본문에 반영됨 (버그 B 수정)", body_grew))
    results.append(("본문 입력이 제목을 건드리지 않음", title_untouched))
    print(f"    {'✅' if body_grew else '❌'} 본문 증가")
    print(f"    {'✅' if title_untouched else '❌'} 제목 불변 (섞이지 않음)")
    print(f"    → bodyOk={st.get('bodyOk')} (본문 편집기 판정)")

    # ── 6) select_body → 본문 편집기를 골라야 함 ──
    print("\n[6] blog_cmd select_body — 본문 편집기 선택 (폴링)")
    print(f"    ack: {a.blog_cmd('select_body')}")
    time.sleep(2.0)
    sel_probe = a.eval_json(r"""(() => {
      var s = getSelection();
      if (!s.rangeCount) return JSON.stringify({selected:false});
      var r = s.getRangeAt(0);
      var node = r.commonAncestorContainer;
      if (node.nodeType === 3) node = node.parentNode;
      var inTitle = !!(node.closest && node.closest('.se-title-text'));
      var inBody  = !!(node.closest && node.closest('.se-section-container'));
      return JSON.stringify({selected:true, text_len:(s.toString()||'').length,
                             in_title:inTitle, in_body:inBody});
    })()""")
    print(f"    {sel_probe}")
    sel_ok = bool(sel_probe.get("in_body")) and not bool(sel_probe.get("in_title"))
    results.append(("select_body가 본문 편집기를 선택함 (버그 B 수정)", sel_ok))
    print(f"    {'✅' if sel_ok else '❌'} 선택이 본문 안에 있음")

    # ── 7) font_size scope=body → **값 변화** (span 개수 아님) ──
    print("\n[7] blog_cmd font_size size=7 scope=body — 본문 전체 폰트")
    print(f"    ack: {a.blog_cmd('font_size', size=7, scope='body')}")
    st, ms = wait_status(a, lambda s: counter(s, "bodyFontSize") != b_body_px, timeout=20)
    a_body_px, a_title_px = counter(st, "bodyFontSize"), counter(st, "titleFontSize")
    a_body_fs = counter(st, "bodyFontSpans")
    print(f"    {ms:.0f}ms 경과, bodyFontSpans(진단용)={a_body_fs}")
    print(f"    ★ 판정 신호 — bodyFontSize **값**: {b_body_px} → {a_body_px} px")
    print(f"    titleFontSize: {b_title_px} → {a_title_px} px (본문 폰트가 건드리면 안 됨)")
    font_body_ok = a_body_px != b_body_px and a_body_px > 0
    title_untouched_px = a_title_px == b_title_px
    results.append(("font_size scope=body가 본문 font-size 값을 바꿈 (버그 C 수정)", font_body_ok))
    results.append(("본문 폰트가 제목 값을 건드리지 않음", title_untouched_px))
    print(f"    {'✅' if font_body_ok else '❌'} 본문 값 변화 — 개수가 아니라 **값**으로 판정")
    print(f"    {'✅' if title_untouched_px else '❌'} 제목 값 불변")
    print(f"    → 참고: 개수 카운터는 진단용. 이미 스타일된 요소의 크기 변경은 개수에 안 잡힌다")

    # ── 8) font_size scope=title → 제목 값 변화 ──
    print("\n[8] blog_cmd font_size size=3 scope=title — 제목 폰트")
    print(f"    ack: {a.blog_cmd('font_size', size=3, scope='title')}")
    st, ms = wait_status(a, lambda s: counter(s, "titleFontSize") != a_title_px, timeout=20)
    t_px2 = counter(st, "titleFontSize")
    print(f"    {ms:.0f}ms 경과")
    print(f"    ★ 판정 신호 — titleFontSize **값**: {a_title_px} → {t_px2} px")
    font_title_ok = t_px2 != a_title_px and t_px2 > 0
    results.append(("font_size scope=title가 제목 font-size 값을 바꿈", font_title_ok))
    print(f"    {'✅' if font_title_ok else '❌'} 제목 값 변화")
    if not font_title_ok:
        print("    → 값이 안 바뀌었다면 execCommand가 그 span에 실제로 적용을 못 한 것 —")
        print("      개수 판정이었을 땐 이것과 '판정 불가'가 구분이 안 됐다. 이제는 정직하게 실패로 보고된다.")

    # ── 9) 미결 질문: keyup 누락이 엔진 결함인가 ──
    # body에 type_human으로 다시 치면서, mock 페이지가 키 값 배열로 기록하게 둔다.
    print("\n[9] 미결 질문 — keyup이 적게 잡히는 게 엔진 결함인가")
    print("    mock이 편집기별 keydown/keyup **키 값 배열**을 기록한다 —")
    print("    누락된 키를 정확히 지목할 수 있다 (프로브 경쟁 여부 판정 가능)")
    # 계측 초기화 후 **본문을 포커스**한 뒤 type_human 실행.
    # ⚠️ 소켓 `type_human` 은 `document.activeElement` 에 친다 — 셀렉터 파라미터가
    # 없다. 그래서 포커스를 먼저 본문으로 잡아주지 않으면 제목에 입력된다.
    # (블로그 초안 경로는 이 op을 안 쓰고 `blog_cmd type_text` 를 쓴다 — 그쪽은
    #  BODY_EDITABLE_JS 로 본문을 명시 타깃팅한다.)
    print(f"    {a.eval_sync(FOCUS_BODY_JS)}")
    TYPE_PROBE = "cadenceKeyupProbe9"
    print(f"    type_human \"{TYPE_PROBE}\" ({len(TYPE_PROBE)}자)")
    t0 = time.time()
    print(f"    ack: {a.send({'cmd':'type_human','text':TYPE_PROBE})}")
    # 값이 다 채워질 때까지 대기
    while time.time() - t0 < 40:
        time.sleep(0.3)
        got = a.eval_sync("(() => (document.activeElement && document.activeElement.value) || (document.querySelector('#mock-body')||{}).innerText || '')()")
        if len(got or "") >= len(TYPE_PROBE):
            break
    filled_ms = (time.time() - t0) * 1000
    # ⚠️ settle 대기 — 이게 핵심. 값이 다 채워졌다고 마지막 keyup이 이미 온 게 아니다.
    print(f"    값 완성: {filled_ms:.0f}ms")
    print("    settle 대기 1500ms (마지막 keyup이 도착할 시간을 준다) …")
    time.sleep(1.5)
    ie = a.eval_json("(() => window.__ie_read())()")
    print(f"    {json.dumps(ie, ensure_ascii=False)}")
    b = ie.get("body", {})
    kd, ku = b.get("keydown", 0), b.get("keyup", 0)
    missing = b.get("missing_keyup", [])
    gap = b.get("gap_ms", 0)
    n = len(TYPE_PROBE)
    print(f"    본문 keydown={kd} keyup={ku} (텍스트 {n}자), 첫→끝 간격={gap}ms")
    pairing_ok = (kd == ku) and kd >= n
    results.append((f"keyup이 keydown과 동수 (settle 후) — kd={kd}, ku={ku}", pairing_ok))
    results.append((f"사람 리듬 소요시간 (≥1.5s) — {gap}ms", gap >= 1500))
    if pairing_ok:
        print("    ✅ settle 후 keydown/keyup 동수 — **프로브 측정 경쟁**이었다")
        print("       값 완성 순간에 읽어서 마지막 keyup을 놓친 것. 엔진 결합 아님.")
    else:
        print(f"    ❌ settle 후에도 불일치 — 누락된 키: {missing}")
        print("       이 경우 **엔진 결함**으로 볼 근거가 있다 (keyup을 보내지 않음)")

    # ── 리포트 ──
    print("\n" + "=" * 58)
    print("  블로그 초안 경로 라이브 리포트 (mock, 네이버 미접촉)")
    print("=" * 58)
    passed = sum(1 for _, ok in results if ok)
    for name, ok in results:
        print(f"  {'✅' if ok else '❌'} {name}")
    print(f"  ── {passed}/{len(results)} 통과")
    print("\n  ⚠️ 이 검증이 답하지 못하는 것: **실제 네이버 편집기에서의 동작.**")
    print("     mock은 SE ONE의 DOM 구조만 재현한다 — 네이버의 실제 에디터 JS/업로드")
    print("     파이프라인은 다르다. 실계정 검증은 로그인 세션이 필요하고(BCB #31864")
    print("     계정 잠금 이력), 손 로그인 후 세션 위에서만 가능하다.")


if __name__ == "__main__":
    main()
