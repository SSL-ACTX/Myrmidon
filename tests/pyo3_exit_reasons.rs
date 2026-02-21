#![cfg(feature = "pyo3")]

use pyo3::prelude::*;

#[tokio::test]
async fn test_exit_reason_on_panic() {
    Python::with_gil(|py| {
        let module = iris::py::make_module(py).unwrap();
        let rt = module.as_ref(py).getattr("PyRuntime").unwrap().call0().unwrap();

        // Spawn an observed actor that will be stopped normally.
        let target: u64 = rt
            .call_method1("spawn_observed_handler", (10usize,))
            .unwrap()
            .extract()
            .unwrap();

        // Spawn an observer to collect exit notifications
        let observer: u64 = rt
            .call_method1("spawn_observed_handler", (10usize,))
            .unwrap()
            .extract()
            .unwrap();

        // Link the target to the observer so the observer receives the EXIT
        rt.call_method1("link", (target, observer)).unwrap();

        // Stop the target actor (normal exit)
        rt.call_method1("stop", (target,)).unwrap();

        // Give a small delay for the exit to be delivered
        std::thread::sleep(std::time::Duration::from_millis(50));

        let msgs: Vec<pyo3::PyObject> = rt
            .call_method1("get_messages", (observer,))
            .unwrap()
            .extract()
            .unwrap();

        // Find an EXIT message with reason 'normal'
        let mut found = false;
        for m in msgs {
            if let Ok(type_name) = m.as_ref(py).getattr("type_name") {
                if type_name.extract::<String>().unwrap_or_default() == "EXIT" {
                    let reason: String = m.as_ref(py).getattr("reason").unwrap().extract().unwrap_or_default();
                    if reason == "normal" {
                        found = true;
                        break;
                    }
                }
            }
        }

        assert!(found, "expected to find an EXIT message with reason 'normal'");
    });
}
