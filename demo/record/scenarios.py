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


SCENARIOS = {
    "blog-informed": ("blog", blog_informed),
    "blog-naive": ("blog", blog_naive),
    "slides-informed": ("slides", slides_informed),
}
