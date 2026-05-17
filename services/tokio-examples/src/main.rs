use anyhow::Result;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

async fn run_child_process(cmd: &str, dur: Duration) -> Result<()> {
    info!(%cmd, "starting child");

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout captured");
    let stderr = child.stderr.take().expect("stderr captured");

    let stdout_task = tokio::spawn(async move {
        let mut rdr = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = rdr.next_line().await {
            info!(%line, "child stdout");
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut rdr = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = rdr.next_line().await {
            warn!(%line, "child stderr");
        }
    });

    match timeout(dur, child.wait()).await {
        Ok(status_res) => {
            let status = status_res?;
            info!(?status, "child exited");
        }
        Err(_) => {
            warn!("child timed out, killing...");
            let _ = child.kill().await;
        }
    }

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing subscriber with env filter
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("tokio mid-level example: starting");

    let (tx, mut rx) = mpsc::channel::<String>(2); // bounded channel to show backpressure

    // producer
    let prod = tokio::spawn(async move {
        for i in 0..5 {
            let msg = format!("msg-{}", i);
            match tx.try_send(msg.clone()) {
                Ok(_) => info!(%msg, "producer try_send succeeded"),
                Err(e) => {
                    debug!(error = %e, "producer try_send failed, awaiting send");
                    if tx.send(msg.clone()).await.is_err() {
                        warn!("receiver dropped, stopping producer");
                        return;
                    }
                }
            }
            sleep(Duration::from_millis(150)).await;
        }
        info!("producer finished")
    });

    // consumer
    let cons = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            info!(%m, "consumer got");
            sleep(Duration::from_millis(300)).await; // simulate work
        }
        info!("consumer exiting");
    });

    // run a child process with timeout
    let child_run = tokio::spawn(async {
        // command prints lines slowly
        let cmd = "for i in 1 2 3; do echo child-line-$i; sleep 0.2; done";
        let _ = run_child_process(cmd, Duration::from_secs(2)).await;
    });

    // graceful shutdown: wait for ctrl_c or tasks to finish
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("received Ctrl+C, shutting down");
        }
        _ = prod => {
            info!("producer task completed");
        }
    }

    // ensure receiver finishes
    // drop the sender by letting prod finish and dropping tx

    let _ = cons.await;
    let _ = child_run.await;

    info!("tokio mid-level example: finished");
    Ok(())
}
