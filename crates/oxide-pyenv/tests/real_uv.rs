//! The Phase 12 exit check against the real `uv`: from nothing to a chart.
//!
//! Needs the network (it downloads CPython and the packages), so it is ignored by default.
//! `scripts\win\phase12.ps1` runs it with `OXIDE_UV` pointing at the pinned sidecar:
//! `cargo test -p oxide-pyenv --test real_uv -- --ignored --nocapture`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oxide_pyenv::{NoSink, PythonConfig, PythonEnv, RunRequest};

const CHART: &str = r#"
import matplotlib
import numpy as np
import pandas as pd
import openpyxl
import matplotlib.pyplot as plt

df = pd.DataFrame({"x": np.arange(5), "y": np.arange(5) ** 2})
df.to_csv("table.csv", index=False)
plt.plot(df["x"], df["y"])
plt.savefig("chart.png")
print("pandas", pd.__version__, "openpyxl", openpyxl.__version__)
"#;

#[tokio::test]
#[ignore = "downloads Python and the packages"]
async fn a_fresh_profile_gets_python_and_a_chart() {
    let uv = std::env::var_os("OXIDE_UV")
        .map(PathBuf::from)
        .expect("set OXIDE_UV to the uv executable");
    let root = tempfile::tempdir().expect("a temporary folder");
    let mut cfg = PythonConfig::new(root.path().join("python"));
    cfg.uv = Some(uv);
    let env = PythonEnv::new(cfg, Arc::new(NoSink));

    assert!(env.ready().await.is_none(), "nothing is installed yet");
    let work = root.path().join("work");
    let out = env
        .run(RunRequest {
            code: CHART.to_owned(),
            cwd: work.clone(),
            timeout: Duration::from_secs(120),
        })
        .await
        .expect("the first run installs Python and runs the script");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.exit_code, Some(0), "stderr: {stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("pandas"));
    let png = std::fs::read(work.join("chart.png")).expect("the chart was written");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "chart.png is a PNG");
    assert!(work.join("table.csv").exists());

    // Installed now: the next run is quick and the status lists the packages.
    assert!(env.ready().await.is_some());
    let status = env.status(false).await;
    assert!(status.packages.iter().any(|p| p.name == "pandas"));

    // A reset puts it back to nothing.
    env.reset().await.expect("reset works");
    assert!(env.ready().await.is_none());
}
