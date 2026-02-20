import time
from myrmidon import Runtime


def test_path_registry_and_watch():
    rt = Runtime()

    a = rt.spawn(lambda m: None, 10)
    b = rt.spawn(lambda m: None, 10)

    rt.register_path("/system/service/one", a)
    rt.register_path("/system/service/two", b)

    assert rt.whereis_path("/system/service/one") == a
    assert rt.whereis_path("/system/service/two") == b

    children = rt.list_children("/system/service")
    paths = [p for p, _ in children]
    assert "/system/service/one" in paths
    assert "/system/service/two" in paths

    direct = rt.list_children_direct("/system/service")
    paths_direct = [p for p, _ in direct]
    assert "/system/service/one" in paths_direct
    assert "/system/service/two" in paths_direct

    c = rt.spawn(lambda m: None, 10)
    rt.register_path("/system/service/two/grand", c)

    all_under_two = rt.list_children("/system/service/two")
    assert any(p == "/system/service/two/grand" for p, _ in all_under_two)

    direct_under_two = rt.list_children_direct("/system/service/two")
    assert any(p == "/system/service/two/grand" and pid == c for p, pid in direct_under_two)

    # ensure direct listing at the higher prefix doesn't show the grand child
    direct_under_service = rt.list_children_direct("/system/service")
    assert not any(p == "/system/service/two/grand" for p, _ in direct_under_service)

    # Test watch_path: ensure children become supervised
    rt.watch_path("/system/service")

    # small sleep to let supervisor tasks register
    time.sleep(0.05)

    cps = rt.child_pids()
    assert a in cps
    assert b in cps

