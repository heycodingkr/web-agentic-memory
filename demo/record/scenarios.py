"""Scripted agent runs against the demo editors. Each function gets an Agent and logs cues for the edit.

"informed" runs follow a short memory (the hints are listed in the edit); "naive" runs show the detours that
agents typically take without one. The pacing is scripted for readability: it is not a measurement.
"""

BLOG_TITLE = "Why shared trails beat wandering alone"
BLOG_P1 = "Every site hides small surprises: popups, hidden buttons, editors with opinions."
BLOG_P2 = "Once one agent finds the way, the next one simply follows."

SLIDE1_TITLE = "Shared trails for web agents"
SLIDE1_SUB = "Learn it once. Everyone benefits."
SLIDE2_TITLE = "Why agents get lost"
SLIDE2_POINTS = ["Buttons hide below the fold", "Editors reject plain input", "Popups block the way"]
SLIDE3_TITLE = "Follow the trail"
SLIDE3_POINT = "Share the shortcut"

DOC_NAME = "Shared trails brief"
DOC_GOAL = "Goal"
DOC_P1 = "Help web agents reuse what others already learned."
DOC_PLAN = "Plan"
DOC_BULLETS = ["Record the trail", "Share the hint", "Reuse it next time"]
DOC_TIMELINE = "Timeline"
DOC_TABLE = [["Week", "Task", "Owner"], ["1", "Record", "Ana"], ["2", "Share", "Ben"]]
CARET = "window.__demo.caretBlock()"  # the editor is one page-sized box: outline the block being typed in


def blog_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("popup")
    a.click("#newBtn")
    a.pause(0.4)
    a.cue("title")
    a.click("#title")
    a.type(BLOG_TITLE)
    a.cue("title_done")
    a.pause(0.3)
    a.click("#body")
    a.cue("p1")
    a.type(BLOG_P1)
    a.cue("p1_done")
    a.key("Enter")  # one Enter between paragraphs
    a.cue("p2")
    a.type(BLOG_P2)
    a.cue("p2_done")
    a.pause(0.6)
    a.cue("preview")
    a.click("#previewBtn")
    a.pause(2.2)
    a.cue("save")
    a.click("#saveBtn")
    a.pause(0.5)
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.snapshot()")
    assert state["title"] == BLOG_TITLE and len(state["paragraphs"]) == 2 and state["emptyParagraphs"] == 0, state
    assert a.read("window.__demo.saved() !== null"), "draft was not saved"


def blog_naive(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.8)
    a.cue("title")
    a.click("#title")  # the popup is still open: the click lands on the backdrop
    a.type(BLOG_TITLE[:14])  # and the keystrokes go nowhere
    a.pause(1.2)
    a.cue("popup")
    a.click("#newBtn")
    a.pause(0.4)
    a.cue("retitle")
    a.click("#title")
    a.type(BLOG_TITLE)
    a.pause(0.3)
    a.click("#body")
    a.cue("p1")
    a.type(BLOG_P1)
    a.key("Enter", times=2)  # two Enters: an empty paragraph appears between the two
    a.cue("gap")
    a.type(BLOG_P2)
    a.pause(0.6)
    a.cue("save")
    a.click("#saveBtn")
    a.pause(1.7)
    a.cue("fix")
    a.click("#body p.empty-block")
    a.key("Backspace")
    a.pause(0.6)
    a.cue("resave")
    a.click("#saveBtn")
    a.pause(0.5)
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.snapshot()")
    assert state["title"] == BLOG_TITLE and len(state["paragraphs"]) == 2 and state["emptyParagraphs"] == 0, state


def slides_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("s1_title")
    a.click("#scaler .slide .t")
    a.type(SLIDE1_TITLE)
    a.cue("s1_sub")
    a.click("#scaler .slide .b")
    a.type(SLIDE1_SUB)
    a.pause(0.4)
    a.cue("add2")
    a.click("#addSlide")
    a.pause(0.4)
    a.cue("s2_title")
    a.click("#scaler .slide .t")
    a.type(SLIDE2_TITLE)
    a.cue("s2_body")
    a.click("#scaler .slide .b")
    for i, point in enumerate(SLIDE2_POINTS):
        if i:
            a.key("Enter")
        a.type(point, cps=46)
    a.pause(0.5)
    a.cue("add3")
    a.click("#addSlide")
    a.pause(0.5)
    a.cue("s3_title")
    a.click("#scaler .slide .t")
    a.type(SLIDE3_TITLE)
    a.cue("s3_body")
    a.click("#scaler .slide .b")
    a.type(SLIDE3_POINT, cps=46)
    a.pause(0.4)
    a.cue("image")
    a.click("#imageBtn")
    a.pause(0.7)
    a.click('#pickerGrid button[data-p="0"]')
    a.pause(0.8)
    a.cue("oob")
    a.pause(1.3)
    a.cue("resize")
    scale = a.read("document.getElementById('scaler').getBoundingClientRect().width / 960")
    a.drag("#scaler .img .h.se", -120 * scale, -68 * scale, duration=1.0)
    a.cue("resized")
    a.pause(0.4)
    a.cue("saving")
    a.wait_for("window.__demo.saved()", timeout=6)
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.state()")
    flags = a.read("window.__demo.outOfBounds()")
    assert len(state) == 3 and state[0]["title"] == SLIDE1_TITLE and state[2]["image"], state
    assert not any(flags), flags


def _heading(a, text):
    """Headings come from the Styles menu; the Enter that follows drops back to normal text."""
    a.click("#styleBtn")
    a.pause(0.3)
    a.click('#styleMenu [data-style="h1"]')
    a.type(text, mark=CARET)
    a.key("Enter")


def docs_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("name")
    a.click("#docName")
    a.type(DOC_NAME)
    a.key("Enter")  # commits the name
    a.cue("name_done")
    a.pause(0.3)
    a.click("#doc", highlight=False)  # the whole page is the click target: an outline around it would swamp the shot
    a.cue("h1_goal")
    _heading(a, DOC_GOAL)
    a.cue("p1")
    a.type(DOC_P1, mark=CARET)
    a.key("Enter")
    a.cue("h1_plan")
    _heading(a, DOC_PLAN)
    a.cue("bullets")
    a.click("#bulletBtn")
    for i, item in enumerate(DOC_BULLETS):
        if i:
            a.key("Enter")
        a.type(item, cps=46, mark=CARET)
    a.key("Enter", times=2)  # the second Enter, on the empty bullet, leaves the list
    a.cue("h1_time")
    _heading(a, DOC_TIMELINE)
    a.cue("table")
    a.click("#insertBtn")
    a.pause(0.3)
    a.click("#tableItem")
    a.pause(0.3)
    a.click('#tableGrid button[data-r="3"][data-c="3"]')
    a.pause(0.5)
    a.cue("fill")
    cells = [cell for row in DOC_TABLE for cell in row]
    for i, cell in enumerate(cells):
        if i:
            a.key("Tab", gap=0.1)  # Tab moves to the next cell; none after the last one, which would add a row
        a.type(cell, cps=46, mark=CARET if i == 0 else None)
    a.cue("filled")
    a.wait_for("window.__demo.isSaved()", timeout=6)  # the status trails the last keystroke by about a second
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.state()")
    expected = [
        {"t": "h1", "text": DOC_GOAL}, {"t": "p", "text": DOC_P1}, {"t": "h1", "text": DOC_PLAN},
        {"t": "ul", "items": DOC_BULLETS}, {"t": "h1", "text": DOC_TIMELINE}, {"t": "table", "rows": DOC_TABLE},
    ]
    assert state["name"] == DOC_NAME and state["blocks"] == expected, state


SHEET_ROWS = [
    ["Item", "Jan", "Feb", "Total"],
    ["Rent", "1200", "1200", "=SUM(B2:C2)"],
    ["Food", "450", "480", "=SUM(B3:C3)"],
    ["Travel", "150", "90", "=SUM(B4:C4)"],
    ["Total", "=SUM(B2:B4)", "=SUM(C2:C4)", "=SUM(D2:D4)"],
]
SHEET_TOTALS = {"D2": "2,400", "D3": "930", "D4": "240", "B5": "1,800", "C5": "1,770", "D5": "3,570"}
CELL = "window.__demo.cellEl()"  # the grid has no focused element: outline the selected cell instead


def _row(a, cells, cues=None):
    """One spreadsheet row: Tab between cells, then Enter, which drops to the first cell of the next row."""
    for i, text in enumerate(cells):
        if i:
            a.key("Tab", gap=0.12)
        if cues and i in cues:
            a.cue(cues[i])
        a.type(text, cps=46, mark=CELL)
    a.key("Enter", gap=0.25)


def sheets_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("header")
    a.click('[data-cell="A1"]')
    _row(a, SHEET_ROWS[0])
    a.cue("rows")
    _row(a, SHEET_ROWS[1], cues={3: "formula"})
    _row(a, SHEET_ROWS[2])
    _row(a, SHEET_ROWS[3])
    a.cue("total")
    _row(a, SHEET_ROWS[4])
    a.cue("filled")
    a.wait_for("window.__demo.isSaved()", timeout=6)
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.state()")
    expected = {f"{'ABCD'[c]}{r + 1}": text for r, row in enumerate(SHEET_ROWS) for c, text in enumerate(row)}
    assert state["cells"] == expected, state
    assert all(state["shown"][cell] == text for cell, text in SHEET_TOTALS.items()), state


MAIL_TO = ["ana@example.com", "ben@example.com"]
MAIL_SUBJECT = "Trail demo on Friday"
MAIL_LINES = ["Hi Ana and Ben,", "The trail demo is on Friday at 10:00. Bring a laptop."]
CARET_LINE = "window.__demo.caretLine()"


def docs_naive(a):
    """The same brief without hints: three detours (title in the page, # for a heading, - for a list), then the same result."""
    a.pause(0.8)
    a.cue("task")
    a.pause(1.8)
    a.cue("body")
    a.click("#doc", highlight=False)
    a.type(DOC_NAME, mark=CARET)  # the title goes into the page, not the name
    a.key("Enter")
    a.pause(1.0)
    a.cue("untitled")  # the top bar still says Untitled document
    a.pause(1.0)
    a.key("Backspace", times=len(DOC_NAME) + 1, gap=0.03)
    a.cue("rename")
    a.click("#docName")
    a.type(DOC_NAME)
    a.key("Enter")
    a.pause(0.3)
    a.click("#doc", highlight=False)
    a.cue("hash")
    a.type("# " + DOC_GOAL, mark=CARET)  # expects a heading
    a.pause(1.2)
    a.cue("hash_bad")
    a.pause(1.0)
    a.key("Backspace", times=len(DOC_GOAL) + 2, gap=0.05)
    a.cue("redo_goal")
    _heading(a, DOC_GOAL)
    a.cue("p1")
    a.type(DOC_P1, mark=CARET)
    a.key("Enter")
    _heading(a, DOC_PLAN)
    a.cue("dash")
    a.type("- " + DOC_BULLETS[0], cps=46, mark=CARET)  # expects a list
    a.pause(1.2)
    a.cue("dash_bad")
    a.pause(0.8)
    a.key("Backspace", times=len(DOC_BULLETS[0]) + 2, gap=0.04)
    a.cue("redo_bullets")
    a.click("#bulletBtn")
    for i, item in enumerate(DOC_BULLETS):
        if i:
            a.key("Enter")
        a.type(item, cps=46, mark=CARET)
    a.key("Enter", times=2)
    _heading(a, DOC_TIMELINE)
    a.cue("table")
    a.click("#insertBtn")
    a.pause(0.3)
    a.click("#tableItem")
    a.pause(0.3)
    a.click('#tableGrid button[data-r="3"][data-c="3"]')
    a.pause(0.5)
    a.cue("fill")
    cells = [cell for row in DOC_TABLE for cell in row]
    for i, cell in enumerate(cells):
        if i:
            a.key("Tab", gap=0.1)
        a.type(cell, cps=46, mark=CARET if i == 0 else None)
    a.cue("filled")
    a.wait_for("window.__demo.isSaved()", timeout=6)
    a.cue("saved")
    a.pause(2.2)
    a.cue("end")
    state = a.read("window.__demo.state()")
    expected = [
        {"t": "h1", "text": DOC_GOAL}, {"t": "p", "text": DOC_P1}, {"t": "h1", "text": DOC_PLAN},
        {"t": "ul", "items": DOC_BULLETS}, {"t": "h1", "text": DOC_TIMELINE}, {"t": "table", "rows": DOC_TABLE},
    ]
    assert state["name"] == DOC_NAME and state["blocks"] == expected, state


def email_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("to")
    a.click("#to")
    for address in MAIL_TO:
        a.type(address, cps=46)
        a.key("Enter", gap=0.35)  # Enter turns the typed address into a chip
    a.cue("to_done")
    a.pause(0.3)
    a.cue("subject")
    a.click("#subject")
    a.type(MAIL_SUBJECT)
    a.pause(0.3)
    a.cue("body")
    a.click("#msg .first")  # the first line, above the signature
    a.type(MAIL_LINES[0], mark=CARET_LINE)
    a.key("Enter")
    a.type(MAIL_LINES[1], cps=46, mark=CARET_LINE)
    a.pause(0.6)
    a.cue("save")
    a.click("#saveBtn")
    a.pause(0.5)
    a.cue("saved")
    a.pause(2.4)
    a.cue("end")
    state = a.read("window.__demo.state()")
    assert state == {"to": MAIL_TO, "subject": MAIL_SUBJECT, "lines": MAIL_LINES, "below": []}, state
    assert a.read("window.__demo.saved() !== null"), "draft was not saved"


FORM = {"name": "Ana Kim", "email": "ana@example.com", "type": "vacation", "date": "Nov 12, 2026", "note": "Family trip. Back on the 16th.", "confirmed": True}


def forms_informed(a):
    a.pause(0.8)
    a.cue("task")
    a.pause(1.9)
    a.cue("recall")
    a.pause(1.5)
    a.cue("hints")
    a.pause(2.6)
    a.cue("name")
    a.click("#fullName")
    a.type(FORM["name"])
    a.click("#email")
    a.type(FORM["email"], cps=46)
    a.cue("type")
    a.click("#typeBtn")  # a custom list: pick the option, typing selects nothing
    a.pause(0.3)
    a.click('#typeList [data-v="vacation"]')
    a.cue("date")
    a.click("#dateInput")  # read-only: the calendar sets it, and it opens on October
    a.pause(0.3)
    a.click("#calNext")
    a.pause(0.3)
    a.click('#days [data-day="12"]')
    a.cue("note")
    a.click("#note")
    a.type(FORM["note"], cps=46)
    a.cue("confirm")
    a.click("#confirm")  # the required box sits at the very bottom of the form
    a.pause(0.4)
    a.cue("save")
    a.click("#saveBtn")
    a.pause(0.5)
    a.cue("saved")
    a.pause(2.4)
    a.cue("end")
    assert a.read("window.__demo.state()") == FORM, a.read("window.__demo.state()")
    assert a.read("window.__demo.saved() !== null"), "draft was not saved"


SCENARIOS = {
    "blog-informed": ("blog", blog_informed),
    "blog-naive": ("blog", blog_naive),
    "slides-informed": ("slides", slides_informed),
    "docs-informed": ("docs", docs_informed),
    "docs-naive": ("docs", docs_naive),
    "sheets-informed": ("sheets", sheets_informed),
    "email-informed": ("email", email_informed),
    "forms-informed": ("forms", forms_informed),
}
