use iris::Runtime;

#[test]
fn path_registration_and_listing() {
    let rt = Runtime::new();

    // Spawn two observed actors and register them under hierarchical paths
    let a = rt.spawn_observed_handler(10);
    let b = rt.spawn_observed_handler(10);

    rt.register_path("/system/service/one".to_string(), a);
    rt.register_path("/system/service/two".to_string(), b);

    // Exact lookup
    let pa = rt.whereis_path("/system/service/one");
    assert_eq!(pa, Some(a));

    // List all descendants under prefix
    let children = rt.list_children("/system/service");
    assert!(children.iter().any(|(p, pid)| p == "/system/service/one" && *pid == a));
    assert!(children.iter().any(|(p, pid)| p == "/system/service/two" && *pid == b));

    // Direct children (one level) should return the same here
    let direct = rt.list_children_direct("/system/service");
    assert!(direct.iter().any(|(p, pid)| p == "/system/service/one" && *pid == a));
    assert!(direct.iter().any(|(p, pid)| p == "/system/service/two" && *pid == b));

    // Now add a deeper path and ensure direct listing filters it out
    let c = rt.spawn_observed_handler(10);
    rt.register_path("/system/service/two/grand".to_string(), c);

    let all_under_two = rt.list_children("/system/service/two");
    assert!(all_under_two.iter().any(|(p, pid)| p == "/system/service/two/grand" && *pid == c));

    let direct_under_two = rt.list_children_direct("/system/service/two");
    // direct_under_two should include the grand child (it's one level under /system/service/two)
    assert!(direct_under_two.iter().any(|(p, pid)| p == "/system/service/two/grand" && *pid == c));

    // ensure direct listing at the higher prefix doesn't show the grand child
    let direct_under_service = rt.list_children_direct("/system/service");
    assert!(!direct_under_service.iter().any(|(p, _)| p == "/system/service/two/grand"));

    // Test watch_path: ensure children become supervised
    rt.watch_path("/system/service");
    let children = rt.list_children_direct("/system/service");
    for (_p, pid) in children {
        assert!(rt.supervisor().contains_child(pid));
    }
}
