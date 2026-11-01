#!/usr/bin/env python3
"""Draws the architecture overview of Web Agentic Memory as SVG, and renders PNGs with the local Chrome.

    python3 docs/assets/build_architecture_overview.py            # write the SVGs
    python3 docs/assets/build_architecture_overview.py --check    # measure that no text overflows its box
    python3 docs/assets/build_architecture_overview.py --png      # write the SVGs, then render the PNGs

Standard library only. The picture is drawn from code, so every label stays exact and the statuses can follow the
repository: when a planned part gets built, change its `planned` flag below and run the script again.
The PNG is what the READMEs show (it looks the same everywhere); the SVG is the editable vector master.
Chrome runs headless in a throwaway profile; set WAM_CHROME_BIN to use another browser binary.
"""
import argparse
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
W = 1650

INK, MUTED, FAINT = "#1B2430", "#586274", "#8A94A6"
PAPER, WHITE = "#F6F5F1", "#FFFFFF"
HAIR = "#D7DCE4"
PALETTE = {
    # name: (strong, background, line)
    "blue": ("#2563EB", "#EAF2FF", "#A9C7FF"),
    "violet": ("#6D4AFF", "#F2EEFF", "#C4B8FF"),
    "green": ("#0E9F6E", "#E7F8F0", "#93DDBD"),
    "amber": ("#C2760A", "#FFF4DD", "#F0CB84"),
    "slate": ("#475569", "#EFF2F6", "#C3CBD6"),
}

# Statuses of the parts that the repository does not have yet. Flip a flag when it is built.
PLANNED = {"http_provider": False, "postgres": False}

TEXT = {
    "en": {
        "lang": "en",
        "alt": "Architecture overview of Web Agentic Memory: agents recall verified workflows by URL and keywords "
        "(read path), and contributions flow through a queue, a model-driven curator and server-side rules into a "
        "layered memory catalog (write path).",
        "title": "Web Agentic Memory",
        "subtitle": "Shared, verified workflow memory that browser agents recall by URL + keywords",
        "legend_built": "Implemented; locally verified",
        "legend_planned": "Production acceptance is separate",
        "legend_unwired": "See the release acceptance gates",
        "clients": "CLIENTS",
        "browser": ("Chrome · Arise", ["browsers driven by the agent's", "own tools, never by WAM"]),
        "agent": ("Claude Code · Codex", ["or the Arise agent"]),
        "adapters": ("wam CLI · wam-mcp", ["HTTP / stdio · OS credentials", "URL canonicalized on device", "bounded anonymous recall cache"]),
        "arrow_browser": "own tools",
        "arrow_agent": "ask · contribute",
        "people": "PEOPLE",
        "web": ("Web app", ["leaderboard · profile", "consent · my contributions"]),
        "arrow_web": "API",
        "zone": "WEB AGENTIC MEMORY SERVICE",
        "zone_stack": "Rust · axum · wam-core · wam-gatekeeper",
        "read_chip": "READ PATH · recall",
        "read_note": "no model in the read path",
        "r1": ("Recall API", ["URL + keywords in,", "≤ 3 hints back"]),
        "r2": ("Match & rank", ["wam-core · deterministic ranking:", "URL scope → task → keywords"]),
        "arrow_read": "reads L0/L1",
        "write_chip": "WRITE PATH · contribute & curate",
        "write_note": "async · queued · audited",
        "w1": ("Intake", ["explicit consent · closed schema", "URL query, fragment, userinfo stripped"]),
        "w2": ("Durable work", ["lease generation · heartbeat", "consent · retention", "provider quota · retry"]),
        "w3": ("Curator", ["instructions + 10 tools (4 read, 6 decide)", "exact duplicates by rule, no model call", "decides once per job"]),
        "w4": ("Validate & publish", ["scope · consent · lease fence", "trusted replay evidence", "immutable version · score · audit"]),
        "arrow_lease": "claim work",
        "arrow_calls": "tool calls",
        "arrow_apply": "decision",
        "arrow_model": ["propose ⇄", "tool results"],
        "models": "MODEL PROVIDERS",
        "models_note": "one LlmProvider interface · chosen by configuration",
        "m1": ("Meta Model API", ["Muse Spark 1.3 · live-checked"]),
        "m2": ("Claude / Codex CLI", ["local development, no API key"]),
        "m3": ("Scripted fake", ["tests, no model"]),
        "catalog": ("Memory catalog", "versioned · audited"),
        "l0": ("URL + keywords", "index · never injected"),
        "l1": ("Task summary", "≤ 400 estimated tokens"),
        "l2": ("Steps & structure", "opened on demand"),
        "versions": "VERSIONS",
        "v_active": "active",
        "v_old": "superseded",
        "catalog_foot": ["every change is a new immutable", "version; retire = tombstone"],
        "storage": ("PostgreSQL", ["transactional SQL", "local file in development"]),
        "planned": "planned",
        "principles": [
            ["Contributed text is data,", "never instructions"],
            ["The model proposes,", "the server disposes"],
            ["Recall never waits", "on a model"],
            ["Every decision is audited", "and reversible"],
        ],
    },
    "ko": {
        "lang": "ko",
        "alt": "Web Agentic Memory 아키텍처 개요: 에이전트는 URL과 키워드로 검증된 워크플로를 불러오고(읽기 경로), "
        "기여는 큐, 모델이 판정하는 큐레이터, 서버 규칙을 거쳐 레이어드 메모리 카탈로그에 반영된다(쓰기 경로).",
        "title": "Web Agentic Memory",
        "subtitle": "브라우저 에이전트가 URL + 키워드로 불러 쓰는, 검증된 공유 워크플로 메모리",
        "legend_built": "구현됨 · 로컬 검증",
        "legend_planned": "운영 수용 검증은 별도",
        "legend_unwired": "출시 수용 기준 참고",
        "clients": "클라이언트",
        "browser": ("Chrome · Arise", ["브라우저는 에이전트가 자기", "도구로 제어, WAM은 안 건드림"]),
        "agent": ("Claude Code · Codex", ["또는 Arise 에이전트"]),
        "adapters": ("wam CLI · wam-mcp", ["HTTP / stdio · OS credential", "기기에서 URL 정규화", "제한된 익명 recall cache"]),
        "arrow_browser": "자체 도구",
        "arrow_agent": "조회 · 기여",
        "people": "사용자",
        "web": ("웹 앱", ["리더보드 · 프로필", "동의 · 내 기여 현황"]),
        "arrow_web": "API",
        "zone": "WEB AGENTIC MEMORY 서비스",
        "zone_stack": "Rust · axum · wam-core · wam-gatekeeper",
        "read_chip": "읽기 경로 · recall",
        "read_note": "읽기 경로에는 모델이 없다",
        "r1": ("Recall API", ["URL + 키워드를 받아", "힌트 최대 3개 반환"]),
        "r2": ("매칭 · 순위", ["wam-core · 결정적 순위:", "URL 범위 → 업무 → 키워드"]),
        "arrow_read": "L0/L1 조회",
        "write_chip": "쓰기 경로 · 기여와 큐레이션",
        "write_note": "비동기 · 큐 · 감사 기록",
        "w1": ("접수", ["명시적 동의 · 닫힌 스키마", "URL의 쿼리·조각·계정 정보 제거"]),
        "w2": ("영속 작업", ["lease generation · heartbeat", "동의 · 보관기한", "provider quota · 재시도"]),
        "w3": ("큐레이터", ["지침 + 도구 10개 (읽기 4, 결정 6)", "정확히 같은 중복은 규칙으로 처리", "job당 결정은 한 번"]),
        "w4": ("검증 · 게시", ["scope · 동의 · lease fence", "독립 replay 근거", "불변 버전 · 점수 · audit"]),
        "arrow_lease": "작업 lease",
        "arrow_calls": "도구 호출",
        "arrow_apply": "결정 반영",
        "arrow_model": ["제안 ⇄", "도구 결과"],
        "models": "모델 PROVIDER",
        "models_note": "LlmProvider 인터페이스 하나 · 설정으로 선택",
        "m1": ("Meta Model API", ["Muse Spark 1.3 · 실호출 확인"]),
        "m2": ("Claude / Codex CLI", ["로컬 개발, API 키 불필요"]),
        "m3": ("스크립트 fake", ["테스트, 모델 없음"]),
        "catalog": ("메모리 카탈로그", "버전 관리 · 감사 기록"),
        "l0": ("URL + 키워드", "색인용 · 주입하지 않음"),
        "l1": ("업무 요약", "힌트당 추정 ≤ 400토큰"),
        "l2": ("단계 · 구조", "필요할 때만 열람"),
        "versions": "버전",
        "v_active": "활성",
        "v_old": "교체됨",
        "catalog_foot": ["변경은 항상 새 불변 버전,", "폐기는 tombstone"],
        "storage": ("PostgreSQL", ["transactional SQL", "개발: 로컬 snapshot"]),
        "planned": "계획",
        "principles": [
            ["기여된 텍스트는 데이터일 뿐,", "지시가 아니다"],
            ["모델은 제안하고,", "서버가 결정한다"],
            ["recall은 모델을", "기다리지 않는다"],
            ["모든 결정은 기록되고", "되돌릴 수 있다"],
        ],
    },
}

FONT = ('-apple-system,BlinkMacSystemFont,"SF Pro Text","Segoe UI",Roboto,"Helvetica Neue",Arial,'
        '"Apple SD Gothic Neo","Noto Sans KR","Malgun Gothic",sans-serif')
MONO = '"SF Mono",SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace'


def esc(text):
    return html.escape(text, quote=False)


class Canvas:
    def __init__(self, strings):
        self.t = strings
        self.parts = []
        self.fit = []  # static vertical-fit warnings
        self.H = 0

    def add(self, markup):
        self.parts.append(markup)

    # -- primitives ---------------------------------------------------------------------------------------------
    def text(self, x, y, content, size=17, weight=400, fill=MUTED, anchor="start", maxw=None, halo=False,
             spacing=None, mono=False, italic=False):
        attrs = [f'x="{x}"', f'y="{y}"', f'font-size="{size}"', f'font-weight="{weight}"', f'fill="{fill}"']
        if anchor != "start":
            attrs.append(f'text-anchor="{anchor}"')
        if maxw:
            attrs.append(f'data-max="{maxw}"')
        if halo:
            attrs.append(f'stroke="{PAPER}" stroke-width="7" paint-order="stroke" stroke-linejoin="round"')
        if spacing:
            attrs.append(f'letter-spacing="{spacing}"')
        if mono:
            attrs.append(f'font-family=\'{MONO}\'')
        if italic:
            attrs.append('font-style="italic"')
        self.add(f'<text {" ".join(attrs)}>{esc(content)}</text>')

    def rect(self, x, y, w, h, fill, stroke=None, sw=1.5, rx=12, dashed=False, shadow=False, opacity=None):
        attrs = [f'x="{x}"', f'y="{y}"', f'width="{w}"', f'height="{h}"', f'rx="{rx}"', f'fill="{fill}"']
        if stroke:
            attrs.append(f'stroke="{stroke}" stroke-width="{sw}"')
        if dashed:
            attrs.append('stroke-dasharray="8 6"')
        if shadow:
            attrs.append('filter="url(#shadow)"')
        if opacity is not None:
            attrs.append(f'fill-opacity="{opacity}"')
        self.add(f'<rect {" ".join(attrs)}/>')

    def badge(self, x, y, number, color):
        strong = PALETTE[color][0]
        self.add(f'<circle cx="{x}" cy="{y}" r="17" fill="{strong}" stroke="{PAPER}" stroke-width="3"/>')
        self.text(x, y + 6.5, str(number), size=19, weight=800, fill=WHITE, anchor="middle")

    def pill(self, right, y, label, color="slate"):
        """A small status tag whose right edge is at `right`."""
        width = 18 + (8.6 if self.t["lang"] == "en" else 14.5) * len(label)
        self.rect(right - width, y, width, 26, PALETTE[color][1], PALETTE[color][2], 1.2, rx=13)
        self.text(right - width / 2, y + 19, label, size=15, weight=700, fill=PALETTE[color][0], anchor="middle")

    def icon(self, kind, x, y, color):
        strong = PALETTE[color][0]
        s = f'fill="none" stroke="{strong}" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"'
        shapes = {
            "terminal": '<rect x="2" y="4" width="24" height="20" rx="4"/><path d="M7 11l5 4-5 4"/><path d="M15 20h6"/>',
            "browser": '<rect x="2" y="4" width="24" height="20" rx="4"/><path d="M2 10h24"/><path d="M6 7h.01M10 7h.01"/>',
            "plug": '<path d="M10 3v6M18 3v6"/><path d="M6 9h16v4a8 8 0 0 1-16 0z"/><path d="M14 21v5"/>',
            "web": '<rect x="2" y="4" width="24" height="20" rx="4"/><path d="M8 19v-4M14 19V10M20 19v-6"/>',
            "api": '<path d="M10 5C6 5 8 12 4 14c4 2 2 9 6 9"/><path d="M18 5c4 0 2 7 6 9-4 2-2 9-6 9"/>',
            "match": '<circle cx="12" cy="12" r="7"/><path d="M17.5 17.5L25 25"/>',
            "funnel": '<path d="M3 5h22l-8 9v8l-6 3v-11z"/>',
            "queue": '<path d="M4 7h20M4 14h20M4 21h11"/><path d="M19 19l4 2-4 2"/>',
            "spark": '<path d="M14 2l3 9 9 3-9 3-3 9-3-9-9-3 9-3z"/>',
            "shield": '<path d="M14 3l9 3v7c0 6-4 10-9 12-5-2-9-6-9-12V6z"/><path d="M10 14l3 3 5-6"/>',
            "db": '<ellipse cx="14" cy="7" rx="9" ry="3.5"/><path d="M5 7v13c0 2 4 3.5 9 3.5s9-1.5 9-3.5V7"/><path d="M5 13.5c0 2 4 3.5 9 3.5s9-1.5 9-3.5"/>',
            "layers": '<path d="M14 3l11 5-11 5L3 8z"/><path d="M3 14l11 5 11-5"/><path d="M3 20l11 5 11-5"/>',
            "check": '<circle cx="14" cy="14" r="11"/><path d="M9 14l4 4 7-8"/>',
        }
        self.add(f'<g transform="translate({x},{y})" {s}>{shapes[kind]}</g>')

    def card(self, x, y, w, h, title, subs, color="slate", number=None, icon=None, planned=False, tag=None,
             tag_at="bottom", sub_size=17, title_size=23, shadow=True):
        strong, bg, line = PALETTE[color]
        self.rect(x, y, w, h, "#FBFBFD" if planned else WHITE, FAINT if planned else line, 1.8, rx=14,
                  dashed=planned, shadow=shadow and not planned)
        room = w - 40 - (40 if icon else 0) - (0 if tag_at == "bottom" or not tag else 92)
        self.text(x + 20, y + 40, title, size=title_size, weight=700, fill=MUTED if planned else INK, maxw=room)
        line_y = y + 40
        for index, sub in enumerate(subs):
            line_y += 26 if index == 0 else 22
            self.text(x + 20, line_y, sub, size=sub_size, fill=MUTED, maxw=w - 40)
        if line_y > y + h - 12:
            self.fit.append(f"{title}: {len(subs)} lines need {line_y - y + 12}px, card has {h}px")
        if number is not None:
            self.badge(x + 2, y + 2, number, color)
        if tag:
            self.pill(x + w - 14, y + (12 if tag_at == "top" else h - 38), tag)
        elif icon:
            self.icon(icon, x + w - 46, y + 12, color)

    def arrow(self, points, color, double=False, dashed=False, width=3.2, radius=12):
        strong = PALETTE[color][0]
        d = [f"M{points[0][0]} {points[0][1]}"]
        for i in range(1, len(points) - 1):
            (x0, y0), (x1, y1), (x2, y2) = points[i - 1], points[i], points[i + 1]
            r = min(radius, abs(x1 - x0) + abs(y1 - y0), abs(x2 - x1) + abs(y2 - y1))
            ax, ay = x1 - r * _sign(x1 - x0), y1 - r * _sign(y1 - y0)
            bx, by = x1 + r * _sign(x2 - x1), y1 + r * _sign(y2 - y1)
            d.append(f"L{ax} {ay} Q{x1} {y1} {bx} {by}")
        d.append(f"L{points[-1][0]} {points[-1][1]}")
        attrs = [f'd="{" ".join(d)}"', 'fill="none"', f'stroke="{strong}"', f'stroke-width="{width}"',
                 'stroke-linecap="round"', 'stroke-linejoin="round"', f'marker-end="url(#a-{color})"']
        if double:
            attrs.append(f'marker-start="url(#a-{color})"')
        if dashed:
            attrs.append('stroke-dasharray="9 7"')
        self.add(f'<path {" ".join(attrs)}/>')

    def label(self, x, y, content, color, size=16, anchor="middle", maxw=None):
        self.text(x, y, content, size=size, weight=600, fill=PALETTE[color][0], anchor=anchor, halo=True,
                  maxw=maxw)

    # -- the picture --------------------------------------------------------------------------------------------
    def build(self):
        t = self.t
        ax, aw = 40, 290                 # clients column
        bx, bw = ax + aw + 40, 910       # the service
        cx, cw = bx + bw + 40, 290       # the memory catalog
        top = 172

        # geometry of the service zone
        read_y, read_h, read_card_h = top + 52, 176, 108
        read_cy = read_y + 52
        write_y = read_y + read_h + 20
        ch = 148
        row1 = write_y + 52
        row2 = row1 + ch + 58
        write_h = (row2 + ch + 34) - write_y
        zone_h = (write_y + write_h + 18) - top
        c1x, c1w = bx + 40, 380
        c2x, c2w = c1x + c1w + 50, 360
        mid1, mid2 = row1 + ch // 2, row2 + ch // 2
        mb_y, mb_h = top + zone_h + 44, 156
        line_y = mb_y + mb_h + 36
        self.H = line_y + 100

        # title and legend
        self.text(40, 80, t["title"], size=48, weight=800, fill=INK)
        self.text(40, 120, t["subtitle"], size=24, fill=MUTED, maxw=1050)
        lx = W - 40 - 420
        self.rect(lx, 30, 36, 24, WHITE, PALETTE["slate"][2], 1.8, rx=7, shadow=True)
        self.text(lx + 50, 49, t["legend_built"], size=19, fill=MUTED, maxw=370)
        self.text(lx + 50, 85, t["legend_planned"], size=16, fill=MUTED, maxw=370)
        self.text(lx + 50, 121, t["legend_unwired"], size=16, fill=MUTED, maxw=370)

        # the service zone and its two lanes
        self.rect(bx, top, bw, zone_h, WHITE, HAIR, 1.6, rx=24, shadow=True)
        self.text(bx + 24, top + 34, t["zone"], size=16, weight=800, fill=MUTED, spacing=1.6)
        self.text(bx + bw - 24, top + 34, t["zone_stack"], size=16, fill=FAINT, anchor="end", mono=True, maxw=480)
        for (ly, lh, color, chip, note) in (
            (read_y, read_h, "blue", t["read_chip"], t["read_note"]),
            (write_y, write_h, "violet", t["write_chip"], t["write_note"]),
        ):
            strong, bg, line = PALETTE[color]
            self.rect(bx + 20, ly, bw - 40, lh, bg, line, 1.2, rx=18)
            self.text(bx + 44, ly + 32, chip, size=18, weight=800, fill=strong, spacing=0.6, maxw=420)
            self.text(bx + bw - 44, ly + 32, note, size=17, weight=600, fill=strong, anchor="end", italic=True,
                      maxw=330)

        # read lane: 1 Recall API -> 2 Match & rank
        self.card(c1x, read_cy, c1w, read_card_h, *t["r1"], color="blue", number=1, icon="api")
        self.card(c2x, read_cy, c2w, read_card_h, *t["r2"], color="blue", number=2, icon="match")
        read_mid = read_cy + read_card_h // 2
        self.arrow([(c1x + c1w, read_mid), (c2x, read_mid)], "blue")

        # write lane: 3 Intake -> 4 Job queue -> 5 Curator -> 6 Tool layer
        self.card(c1x, row1, c1w, ch, *t["w1"], color="violet", number=3, icon="funnel")
        self.card(c2x, row1, c2w, ch, *t["w2"], color="violet", number=4, icon="queue")
        self.card(c1x, row2, c1w, ch, *t["w3"], color="violet", number=5, icon="spark")
        self.card(c2x, row2, c2w, ch, *t["w4"], color="green", number=6, icon="shield")
        self.arrow([(c1x + c1w, mid1), (c2x, mid1)], "violet")
        qx, cxm = c2x + c2w // 2, c1x + c1w // 2
        y_turn = row1 + ch + (row2 - row1 - ch) // 2
        self.arrow([(qx, row1 + ch), (qx, y_turn), (cxm, y_turn), (cxm, row2)], "violet")
        self.label((qx + cxm) / 2, y_turn - 11, t["arrow_lease"], "violet", maxw=200)
        self.arrow([(c1x + c1w, mid2), (c2x, mid2)], "green")

        # model providers, below the service: they are external to it
        amber = PALETTE["amber"]
        self.rect(bx, mb_y, bw, mb_h, amber[1], amber[2], 1.4, rx=20)
        self.text(bx + 24, mb_y + 34, t["models"], size=16, weight=800, fill=amber[0], spacing=1.6)
        self.text(bx + bw - 24, mb_y + 34, t["models_note"], size=17, weight=600, fill=amber[0], anchor="end",
                  italic=True, maxw=560)
        chip_gap = 22
        chip_w = (bw - 48 - 2 * chip_gap) / 3
        for i, (key, planned) in enumerate((("m1", PLANNED["http_provider"]), ("m2", False), ("m3", False))):
            self.card(round(bx + 24 + i * (chip_w + chip_gap)), mb_y + 54, round(chip_w), 84, *t[key],
                      color="amber", planned=planned, tag=t["planned"] if planned else None, sub_size=16,
                      title_size=19, shadow=False)
        self.arrow([(cxm, row2 + ch), (cxm, mb_y)], "amber", double=True)
        for i, text in enumerate(t["arrow_model"]):
            self.label(cxm + 16, mb_y - 40 + i * 21, text, "amber", anchor="start", maxw=150)

        # the memory catalog
        cat_y, cat_h = read_y, (write_y + write_h) - read_y
        self.rect(cx, cat_y, cw, cat_h, WHITE, PALETTE["slate"][2], 1.8, rx=22, shadow=True)
        self.icon("layers", cx + cw - 50, cat_y + 16, "slate")
        self.text(cx + 22, cat_y + 42, t["catalog"][0], size=24, weight=800, fill=INK, maxw=cw - 80)
        self.text(cx + 22, cat_y + 68, t["catalog"][1], size=17, fill=MUTED, maxw=cw - 44)
        plate_y = cat_y + 94
        for i, (key, color) in enumerate((("l0", "blue"), ("l1", "blue"), ("l2", "slate"))):
            py = plate_y + i * 110
            strong, bg, line = PALETTE[color]
            self.rect(cx + 16, py, cw - 32, 98, bg, line, 1.4, rx=14)
            self.add(f'<circle cx="{cx + 46}" cy="{py + 49}" r="19" fill="{strong}"/>')
            self.text(cx + 46, py + 56, f"L{i}", size=18, weight=800, fill=WHITE, anchor="middle")
            self.text(cx + 76, py + 41, t[key][0], size=19, weight=700, fill=INK, maxw=cw - 96)
            self.text(cx + 76, py + 68, t[key][1], size=16, fill=MUTED, maxw=cw - 96)
        vy = plate_y + 3 * 110 + 18
        self.text(cx + 22, vy, t["versions"], size=15, weight=800, fill=MUTED, spacing=1.4)
        for i, (name, status, color) in enumerate((("v3", t["v_active"], "green"), ("v2", t["v_old"], "slate"),
                                                   ("v1", t["v_old"], "slate"))):
            py = vy + 14 + i * 38
            strong, bg, line = PALETTE[color]
            self.rect(cx + 16, py, cw - 32, 30, bg if i == 0 else "#F5F7FA", line if i == 0 else HAIR, 1.2, rx=10)
            self.text(cx + 32, py + 21, name, size=16, weight=800, fill=strong if i == 0 else FAINT)
            self.text(cx + 72, py + 21, status, size=16, weight=600 if i == 0 else 400,
                      fill=strong if i == 0 else FAINT, maxw=cw - 110)
        for i, text in enumerate(t["catalog_foot"]):
            self.text(cx + 22, cat_y + cat_h - 44 + i * 21, text, size=16, fill=MUTED, maxw=cw - 40)

        # catalog arrows: the read lane reads it, the tool layer writes it
        gap_mid = ((c2x + c2w) + cx) / 2
        self.arrow([(cx, read_mid), (c2x + c2w, read_mid)], "blue")
        self.label(gap_mid, read_mid - 12, t["arrow_read"], "blue", maxw=116)
        self.arrow([(c2x + c2w, mid2), (cx, mid2)], "green")
        self.label(gap_mid, mid2 - 12, t["arrow_apply"], "green", maxw=116)

        # storage under the catalog
        self.card(cx, mb_y, cw, mb_h, *t["storage"], color="slate", planned=PLANNED["postgres"],
                  tag=t["planned"] if PLANNED["postgres"] else None, tag_at="top", icon="db" if not PLANNED["postgres"] else None,
                  title_size=22, shadow=False)
        self.add(f'<path d="M{cx + cw // 2} {cat_y + cat_h} V{mb_y}" stroke="{FAINT}" stroke-width="2.4" '
                 f'stroke-linecap="round" fill="none"/>')

        # clients column: browser <- agent -> adapters, and the people's web app
        self.text(ax, top - 8, t["clients"], size=16, weight=800, fill=MUTED, spacing=1.6)
        b_y, b_h = top + 8, 106
        a_y, a_h = b_y + b_h + 54, 84
        p_y, p_h = a_y + a_h + 54, 140
        self.card(ax, b_y, aw, b_h, *t["browser"], color="slate", icon="browser", sub_size=16, title_size=21)
        self.card(ax, a_y, aw, a_h, *t["agent"], color="slate", icon="terminal", title_size=21)
        self.card(ax, p_y, aw, p_h, *t["adapters"], color="blue", icon="plug", sub_size=16, title_size=21)
        mid_x = ax + aw // 2
        self.arrow([(mid_x, a_y), (mid_x, b_y + b_h)], "slate")
        self.label(mid_x + 14, b_y + b_h + 34, t["arrow_browser"], "slate", anchor="start", maxw=160)
        self.arrow([(mid_x, a_y + a_h), (mid_x, p_y)], "slate")
        self.label(mid_x + 14, a_y + a_h + 34, t["arrow_agent"], "slate", anchor="start", maxw=160)
        bus = ax + aw + 20
        self.arrow([(ax + aw, p_y + 34), (bus, p_y + 34), (bus, read_mid), (c1x, read_mid)], "blue", double=True)
        self.arrow([(ax + aw, mid1), (c1x, mid1)], "violet")
        w_y = p_y + p_h + 64
        self.text(ax, w_y - 16, t["people"], size=16, weight=800, fill=MUTED, spacing=1.6)
        self.card(ax, w_y, aw, 116, *t["web"], color="slate", icon="web", title_size=21, sub_size=16)
        web_mid = w_y + 58
        self.arrow([(ax + aw, web_mid), (bx, web_mid)], "slate")
        self.label(ax + aw + 20, web_mid - 12, t["arrow_web"], "slate", size=15, maxw=40)

        # principles
        self.add(f'<path d="M40 {line_y} H{W - 40}" stroke="{HAIR}" stroke-width="1.5"/>')
        col_w = (W - 80) / 4
        for i, lines in enumerate(t["principles"]):
            x = 40 + i * col_w
            self.icon("check", x, line_y + 28, "green")
            for j, text in enumerate(lines):
                self.text(x + 42, line_y + 42 + j * 25, text, size=20, weight=600, fill=INK, maxw=col_w - 60)
        self.parts.insert(0, f'<rect width="{W}" height="{self.H}" fill="{PAPER}"/>')
        return self

    def svg(self):
        t = self.t
        markers = "".join(
            f'<marker id="a-{name}" markerWidth="14" markerHeight="14" refX="11" refY="7" orient="auto-start-reverse" '
            f'markerUnits="userSpaceOnUse"><path d="M2 1.5L12 7 2 12.5z" fill="{vals[0]}"/></marker>'
            for name, vals in PALETTE.items())
        defs = ('<defs><filter id="shadow" x="-8%" y="-8%" width="116%" height="124%">'
                '<feDropShadow dx="0" dy="3" stdDeviation="4" flood-color="#0F172A" flood-opacity="0.09"/></filter>'
                f'{markers}</defs>')
        style = f'<style>text{{font-family:{FONT};}}</style>'
        return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{self.H}" viewBox="0 0 {W} {self.H}" '
                f'role="img" aria-label="{html.escape(t["alt"], quote=True)}" lang="{t["lang"]}">'
                f'<title>{esc(t["alt"])}</title>{defs}{style}{"".join(self.parts)}</svg>\n')


def _sign(value):
    return (value > 0) - (value < 0)


def chrome_binary():
    for candidate in (os.environ.get("WAM_CHROME_BIN"),
                      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                      shutil.which("google-chrome"), shutil.which("google-chrome-stable"),
                      shutil.which("chromium"), shutil.which("chromium-browser")):
        if candidate and Path(candidate).exists():
            return candidate
    sys.exit("Chrome was not found. Install it or set WAM_CHROME_BIN.")


def run_chrome(args, timeout=90):
    profile = tempfile.mkdtemp(prefix="wam-arch-")
    try:
        command = [chrome_binary(), "--headless=new", "--disable-gpu", "--no-first-run",
                   "--no-default-browser-check", "--hide-scrollbars", f"--user-data-dir={profile}", *args]
        return subprocess.run(command, capture_output=True, text=True, timeout=timeout, start_new_session=True)
    finally:
        shutil.rmtree(profile, ignore_errors=True)


def measure(svg_text):
    """Text that is wider than the box it was written for, measured by the browser itself."""
    page = ('<!doctype html><meta charset="utf-8"><body style="margin:0">' + svg_text +
            '<pre id="r"></pre><script>const out=[];for(const t of document.querySelectorAll("svg text[data-max]")){'
            'const w=t.getComputedTextLength(),m=+t.dataset.max;if(w>m)out.push(t.textContent+" :: "+Math.round(w)+" > "+m);}'
            'document.getElementById("r").textContent="RESULT"+JSON.stringify(out)+"END";</script>')
    with tempfile.TemporaryDirectory(prefix="wam-arch-page-") as folder:
        path = Path(folder) / "check.html"
        path.write_text(page, encoding="utf-8")
        done = run_chrome(["--virtual-time-budget=3000", "--dump-dom", path.as_uri()])
    found = re.search(r"RESULT(.*?)END", done.stdout, re.S)
    if not found:
        sys.exit("could not read the measurement from Chrome:\n" + done.stderr[-400:])
    return json.loads(html.unescape(found.group(1)))  # a JSON list of strings


def render_png(svg_path, png_path, height, scale=2):
    done = run_chrome([f"--window-size={W},{height}", f"--force-device-scale-factor={scale}", f"--screenshot={png_path}",
                       svg_path.as_uri()])
    if not png_path.exists():
        sys.exit("Chrome did not write the PNG:\n" + done.stderr[-400:])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="measure text against its boxes and exit non-zero on overflow")
    parser.add_argument("--png", action="store_true", help="render PNGs with the local Chrome")
    parser.add_argument("--out", default=str(HERE), help="output directory (default: next to this script)")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    problems = []
    for lang, suffix in (("en", ""), ("ko", ".ko")):
        canvas = Canvas(TEXT[lang]).build()
        svg = canvas.svg()
        target = out / f"architecture-overview{suffix}.svg"
        target.write_text(svg, encoding="utf-8")
        print("wrote", target)
        problems += [f"[{lang}] {item}" for item in canvas.fit]
        if args.check:
            problems += [f"[{lang}] overflow: {item}" for item in measure(svg)]
        if args.png:
            png = out / f"architecture-overview{suffix}.png"
            render_png(target, png, canvas.H)
            print("wrote", png, f"{png.stat().st_size // 1024} KiB")
    for item in problems:
        print("PROBLEM", item)
    sys.exit(1 if problems and args.check else 0)


if __name__ == "__main__":
    main()
