import time
from myrmidon import Runtime


def test_path_supervisor_watch_children():
    rt = Runtime()

    pid = rt.spawn(lambda m: None, 10)
    rt.register_path("/svc/test/one", pid)

    rt.create_path_supervisor("/svc/test")
    rt.path_supervisor_watch("/svc/test", pid)

    children = rt.path_supervisor_children("/svc/test")
    assert pid in children

    rt.remove_path_supervisor("/svc/test")
    children2 = rt.path_supervisor_children("/svc/test")
    assert children2 == []
