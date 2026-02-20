// tests/pyo3_path_supervisors.rs
#![cfg(feature = "pyo3")]

use pyo3::prelude::*;
use std::time::Duration;

#[tokio::test]
async fn test_path_supervisor_watch_children() {
    Python::with_gil(|py| {
        let module = myrmidon::py::make_module(py).expect("make_module");
        let rt_type = module.as_ref(py).getattr("PyRuntime").unwrap();
        let rt = rt_type.call0().unwrap();

        // spawn and register under a path using the observed spawn helper
        let pid: u64 = rt
            .call_method1("spawn_with_path_observed", (10usize, "/svc/test/one"))
            .unwrap()
            .extract()
            .unwrap();

        // create a path supervisor and watch the pid
        rt.call_method1("create_path_supervisor", ("/svc/test",)).unwrap();
        rt.call_method1("path_supervisor_watch", ("/svc/test", pid)).unwrap();

        let children: Vec<u64> = rt
            .call_method1("path_supervisor_children", ("/svc/test",))
            .unwrap()
            .extract()
            .unwrap();

        assert!(children.contains(&pid));

        // remove and ensure empty
        rt.call_method1("remove_path_supervisor", ("/svc/test",)).unwrap();
        let children2: Vec<u64> = rt
            .call_method1("path_supervisor_children", ("/svc/test",))
            .unwrap()
            .extract()
            .unwrap();
        assert!(children2.is_empty());
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
}
