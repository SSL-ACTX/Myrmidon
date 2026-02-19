import time
import threading
import pytest

import myrmidon


def test_release_gil_toggle():
    rt = myrmidon.Runtime()

    seen = []

    def handler_no(msg):
        seen.append(("no", threading.get_ident()))

    def handler_yes(msg):
        seen.append(("yes", threading.get_ident()))

    pid_no = rt.spawn(handler_no, budget=10, release_gil=False)
    pid_yes = rt.spawn(handler_yes, budget=10, release_gil=True)

    rt.send(pid_no, b"ping")
    rt.send(pid_yes, b"ping")

    # Allow the handlers time to run
    time.sleep(0.2)

    tags = {t for (t, _) in seen}
    assert "no" in tags and "yes" in tags

    no_tid = next(v for (t, v) in seen if t == "no")
    yes_tid = next(v for (t, v) in seen if t == "yes")

    # Expect different thread ids when using the toggle (best-effort)
    assert no_tid != yes_tid
